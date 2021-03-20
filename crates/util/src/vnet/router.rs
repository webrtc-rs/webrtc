#[cfg(test)]
mod router_test;

use crate::vnet::chunk::*;
use crate::vnet::chunk_queue::*;
use crate::vnet::errors::*;
use crate::vnet::interface::*;
use crate::vnet::nat::*;
use crate::vnet::net::*;
use crate::vnet::resolver::*;
use crate::Error;

use async_trait::async_trait;
use ipnet::*;
use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::ops::{Add, Sub};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Duration;

const DEFAULT_ROUTER_QUEUE_SIZE: usize = 0; // unlimited

lazy_static! {
    pub static ref ROUTER_ID_CTR: AtomicU64 = AtomicU64::new(0);
}

// Generate a unique router name
fn assign_router_name() -> String {
    let n = ROUTER_ID_CTR.fetch_add(1, Ordering::SeqCst);
    format!("router{}", n)
}

// RouterConfig ...
#[derive(Default)]
pub struct RouterConfig {
    // name of router. If not specified, a unique name will be assigned.
    name: String,
    // cidr notation, like "192.0.2.0/24"
    cidr: String,
    // static_ips is an array of static IP addresses to be assigned for this router.
    // If no static IP address is given, the router will automatically assign
    // an IP address.
    // This will be ignored if this router is the root.
    static_ips: Vec<String>,
    // static_ip is deprecated. Use static_ips.
    static_ip: String,
    // Internal queue size
    queue_size: usize,
    // Effective only when this router has a parent router
    nat_type: Option<NATType>,
    // Minimum Delay
    min_delay: Duration,
    // Max Jitter
    max_jitter: Duration,
}

// NIC is a network interface controller that interfaces Router
#[async_trait]
pub trait NIC {
    fn get_interface(&self, ifc_name: &str) -> Option<&Interface>;
    async fn add_addrs_to_interface(
        &mut self,
        ifc_name: &str,
        addrs: &[IpNet],
    ) -> Result<(), Error>;
    async fn on_inbound_chunk(&self, c: Box<dyn Chunk + Send + Sync>);
    fn get_static_ips(&self) -> &[IpAddr];
    async fn set_router(&self, r: Arc<Mutex<Router>>) -> Result<(), Error>;
}

// ChunkFilter is a handler users can add to filter chunks.
// If the filter returns false, the packet will be dropped.
pub type ChunkFilterFn = fn(c: &(dyn Chunk + Send + Sync)) -> bool;

#[derive(Default)]
pub struct RouterInternal {
    nat_type: Option<NATType>,                                // read-only
    ipv4net: IpNet,                                           // read-only
    parent: Option<Arc<Mutex<Router>>>,                       // read-only
    nat: NetworkAddressTranslator,                            // read-only
    nics: HashMap<String, Arc<Mutex<dyn NIC + Send + Sync>>>, // read-only
    chunk_filters: Vec<ChunkFilterFn>,                        // requires mutex [x]
    last_id: u8, // requires mutex [x], used to assign the last digit of IPv4 address
}

// Router ...
#[derive(Default)]
pub struct Router {
    name: String,                              // read-only
    ipv4net: IpNet,                            // read-only
    min_delay: Duration,                       // requires mutex [x]
    max_jitter: Duration,                      // requires mutex [x]
    queue: Arc<ChunkQueue>,                    // read-only
    interfaces: Vec<Interface>,                // read-only
    static_ips: Vec<IpAddr>,                   // read-only
    static_local_ips: HashMap<String, IpAddr>, // read-only,
    children: Vec<Arc<Mutex<Router>>>,         // read-only
    done: Option<mpsc::Sender<()>>,            // requires mutex [x]
    pub(crate) resolver: Arc<Mutex<Resolver>>, // read-only
    push_ch: Option<mpsc::Sender<()>>,         // writer requires mutex
    router_internal: Arc<Mutex<RouterInternal>>,
}

#[async_trait]
impl NIC for Router {
    fn get_interface(&self, ifc_name: &str) -> Option<&Interface> {
        for ifc in &self.interfaces {
            if ifc.name == ifc_name {
                return Some(ifc);
            }
        }
        None
    }

    async fn add_addrs_to_interface(
        &mut self,
        ifc_name: &str,
        addrs: &[IpNet],
    ) -> Result<(), Error> {
        for ifc in &mut self.interfaces {
            if ifc.name == ifc_name {
                for addr in addrs {
                    ifc.add_addr(*addr);
                }
                return Ok(());
            }
        }

        Err(ERR_NOT_FOUND.to_owned())
    }

    async fn on_inbound_chunk(&self, c: Box<dyn Chunk + Send + Sync>) {
        let from_parent: Box<dyn Chunk + Send + Sync> = {
            let router_internal = self.router_internal.lock().await;
            match router_internal.nat.translate_inbound(&*c).await {
                Ok(from) => {
                    if let Some(from) = from {
                        from
                    } else {
                        return;
                    }
                }
                Err(err) => {
                    log::warn!("[{}] {}", self.name, err);
                    return;
                }
            }
        };

        self.push(from_parent).await;
    }

    fn get_static_ips(&self) -> &[IpAddr] {
        &self.static_ips
    }

    // caller must hold the mutex
    async fn set_router(&self, parent: Arc<Mutex<Router>>) -> Result<(), Error> {
        {
            let mut router_internal = self.router_internal.lock().await;
            router_internal.parent = Some(Arc::clone(&parent));
        }

        let parent_resolver = {
            let p = parent.lock().await;
            Arc::clone(&p.resolver)
        };
        {
            let mut resolver = self.resolver.lock().await;
            resolver.set_parent(parent_resolver);
        }

        let mut mapped_ips = vec![];
        let mut local_ips = vec![];

        // when this method is called, one or more IP address has already been assigned by
        // the parent router.
        if let Some(ifc) = self.get_interface("eth0") {
            for ifc_addr in ifc.addrs() {
                let ip = ifc_addr.addr();
                mapped_ips.push(ip);

                if let Some(loc_ip) = self.static_local_ips.get(&ip.to_string()) {
                    local_ips.push(*loc_ip);
                }
            }
        } else {
            return Err(ERR_NO_IPADDR_ETH0.to_owned());
        }

        // Set up NAT here
        {
            let mut router_internal = self.router_internal.lock().await;
            if router_internal.nat_type.is_none() {
                router_internal.nat_type = Some(NATType {
                    mapping_behavior: EndpointDependencyType::EndpointIndependent,
                    filtering_behavior: EndpointDependencyType::EndpointAddrPortDependent,
                    hair_pining: false,
                    port_preservation: false,
                    mapping_life_time: Duration::from_secs(30),
                    ..Default::default()
                });
            }

            router_internal.nat = NetworkAddressTranslator::new(NatConfig {
                name: self.name.clone(),
                nat_type: router_internal.nat_type.unwrap(),
                mapped_ips,
                local_ips,
            })?;
        }

        Ok(())
    }
}

impl Router {
    pub fn new(config: RouterConfig) -> Result<Self, Error> {
        let ipv4net: IpNet = config.cidr.parse()?;

        let queue_size = if config.queue_size > 0 {
            config.queue_size
        } else {
            DEFAULT_ROUTER_QUEUE_SIZE
        };

        // set up network interface, lo0
        let mut lo0 = Interface::new(LO0_STR.to_owned(), vec![]);
        if let Ok(ipnet) = Interface::convert(
            SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 0),
            Some(SocketAddr::new(Ipv4Addr::new(255, 0, 0, 0).into(), 0)),
        ) {
            lo0.add_addr(ipnet);
        }

        // set up network interface, eth0
        let eth0 = Interface::new("eth0".to_owned(), vec![]);

        // local host name resolver
        let resolver = Arc::new(Mutex::new(Resolver::new()));

        let name = if config.name.is_empty() {
            assign_router_name()
        } else {
            config.name.clone()
        };

        let mut static_ips = vec![];
        let mut static_local_ips = HashMap::new();
        for ip_str in &config.static_ips {
            let ip_pair: Vec<&str> = ip_str.split('/').collect();
            if let Ok(ip) = IpAddr::from_str(ip_pair[0]) {
                if ip_pair.len() > 1 {
                    let loc_ip = IpAddr::from_str(ip_pair[1])?;
                    if !ipv4net.contains(&loc_ip) {
                        return Err(ERR_LOCAL_IP_BEYOND_STATIC_IPS_SUBSET.to_owned());
                    }
                    static_local_ips.insert(ip.to_string(), loc_ip);
                }
                static_ips.push(ip);
            }
        }
        if !config.static_ip.is_empty() {
            log::warn!("static_ip is deprecated. Use static_ips instead");
            if let Ok(ip) = IpAddr::from_str(&config.static_ip) {
                static_ips.push(ip);
            }
        }

        let n_static_local = static_local_ips.len();
        if n_static_local > 0 && n_static_local != static_ips.len() {
            return Err(ERR_LOCAL_IP_NO_STATICS_IPS_ASSOCIATED.to_owned());
        }

        let router_internal = RouterInternal {
            nat_type: config.nat_type,
            ipv4net,
            nics: HashMap::new(),
            ..Default::default()
        };

        Ok(Router {
            name,
            ipv4net,
            interfaces: vec![lo0, eth0],
            static_ips,
            static_local_ips,
            resolver,
            router_internal: Arc::new(Mutex::new(router_internal)),
            queue: Arc::new(ChunkQueue::new(queue_size)),
            min_delay: config.min_delay,
            max_jitter: config.max_jitter,
            ..Default::default()
        })
    }

    // caller must hold the mutex
    pub(crate) fn get_interfaces(&self) -> &[Interface] {
        &self.interfaces
    }

    // Start ...
    pub fn start(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        if self.done.is_some() {
            return Box::pin(async move { Err(ERR_ROUTER_ALREADY_STARTED.to_owned()) });
        }

        let (done_tx, mut done_rx) = mpsc::channel(1);
        let (push_ch_tx, mut push_ch_rx) = mpsc::channel(1);
        self.done = Some(done_tx);
        self.push_ch = Some(push_ch_tx);

        let router_internal = Arc::clone(&self.router_internal);
        let queue = Arc::clone(&self.queue);
        let max_jitter = self.max_jitter;
        let min_delay = self.min_delay;
        let name = self.name.clone();
        let ipv4net = self.ipv4net;

        tokio::spawn(async move {
            while let Ok(d) = Router::process_chunks(
                &name,
                ipv4net,
                max_jitter,
                min_delay,
                &queue,
                &router_internal,
            )
            .await
            {
                if d == Duration::from_secs(0) {
                    tokio::select! {
                     _ = push_ch_rx.recv() =>{},
                     _ = done_rx.recv() => break,
                    }
                } else {
                    let t = tokio::time::sleep(d);
                    tokio::pin!(t);

                    tokio::select! {
                    _ = t.as_mut() => {},
                    _ = done_rx.recv() => break,
                    }
                }
            }
        });

        let child_result = Arc::new(AtomicBool::new(false));
        for child in &self.children {
            let child2 = Arc::clone(child);
            let child_result2 = Arc::clone(&child_result);
            Box::pin(async move {
                let mut c = child2.lock().await;
                if c.start().await.is_err() {
                    child_result2.store(true, Ordering::SeqCst);
                }
            });
        }

        Box::pin(async move {
            if child_result.load(Ordering::SeqCst) {
                Err(ERR_ROUTER_ALREADY_STARTED.to_owned())
            } else {
                Ok(())
            }
        })
    }

    // Stop ...
    pub fn stop(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        if self.done.is_none() {
            return Box::pin(async move { Err(ERR_ROUTER_ALREADY_STOPPED.to_owned()) });
        }

        let child_result = Arc::new(AtomicBool::new(false));
        for child in &self.children {
            let child2 = Arc::clone(child);
            let child_result2 = Arc::clone(&child_result);
            Box::pin(async move {
                let mut c = child2.lock().await;
                if c.stop().await.is_err() {
                    child_result2.store(true, Ordering::SeqCst);
                }
            });
        }

        self.push_ch.take();
        self.done.take();

        Box::pin(async move {
            if child_result.load(Ordering::SeqCst) {
                Err(ERR_ROUTER_ALREADY_STOPPED.to_owned())
            } else {
                Ok(())
            }
        })
    }

    // AddRouter adds a chile Router.
    // after parent.add_router(child), also call child.set_router(parent) to set child's parent router
    pub async fn add_router(&mut self, child: Arc<Mutex<Router>>) -> Result<(), Error> {
        // Router is a NIC. Add it as a NIC so that packets are routed to this child
        // router.
        let nic = Arc::clone(&child) as Arc<Mutex<dyn NIC + Send + Sync>>;
        self.children.push(child);
        self.add_net(nic).await
    }

    // AddNet ...
    // after router.add_net(nic), also call nic.set_router(router) to set nic's router
    pub async fn add_net(&mut self, nic: Arc<Mutex<dyn NIC + Send + Sync>>) -> Result<(), Error> {
        let mut router_internal = self.router_internal.lock().await;
        router_internal.add_nic(nic).await
    }

    // AddHost adds a mapping of hostname and an IP address to the local resolver.
    pub async fn add_host(&mut self, host_name: String, ip_addr: String) -> Result<(), Error> {
        let mut resolver = self.resolver.lock().await;
        resolver.add_host(host_name, ip_addr)
    }

    // AddChunkFilter adds a filter for chunks traversing this router.
    // You may add more than one filter. The filters are called in the order of this method call.
    // If a chunk is dropped by a filter, subsequent filter will not receive the chunk.
    pub async fn add_chunk_filter(&self, filter: ChunkFilterFn) {
        let mut router_internal = self.router_internal.lock().await;
        router_internal.chunk_filters.push(filter);
    }

    pub(crate) async fn push(&self, mut c: Box<dyn Chunk + Send + Sync>) {
        log::debug!("[{}] route {}", self.name, c);
        if self.done.is_some() {
            c.set_timestamp();

            if self.queue.push(c).await {
                if let Some(push_ch) = &self.push_ch {
                    let _ = push_ch.try_send(());
                }
            } else {
                log::warn!("[{}] queue was full. dropped a chunk", self.name);
            }
        }
    }

    async fn process_chunks(
        name: &str,
        ipv4net: IpNet,
        max_jitter: Duration,
        min_delay: Duration,
        queue: &Arc<ChunkQueue>,
        router_internal: &Arc<Mutex<RouterInternal>>,
    ) -> Result<Duration, Error> {
        // Introduce jitter by delaying the processing of chunks.
        let mj = max_jitter.as_nanos() as u64;
        if mj > 0 {
            let jitter = Duration::from_nanos(rand::random::<u64>() % mj);
            tokio::time::sleep(jitter).await;
        }

        //      cut_off
        //         v min delay
        //         |<--->|
        //  +------------:--
        //  |OOOOOOXXXXX :   --> time
        //  +------------:--
        //  |<--->|     now
        //    due

        let entered_at = SystemTime::now();
        let cut_off = entered_at.sub(min_delay);

        // the next sleep duration
        let mut d;

        loop {
            d = Duration::from_secs(0);
            if let Some(c) = queue.peek().await {
                // check timestamp to find if the chunk is due
                if c.get_timestamp().duration_since(cut_off).is_ok() {
                    // There is one or more chunk in the queue but none of them are due.
                    // Calculate the next sleep duration here.
                    let next_expire = c.get_timestamp().add(min_delay);
                    if let Ok(diff) = next_expire.duration_since(entered_at) {
                        d = diff;
                        break;
                    }
                }
            } else {
                break; // no more chunk in the queue
            }

            if let Some(c) = queue.pop().await {
                let ri = router_internal.lock().await;
                let mut blocked = false;
                for filter in &ri.chunk_filters {
                    if !filter(&*c) {
                        blocked = true;
                        break;
                    }
                }
                if blocked {
                    continue; // discard
                }

                let dst_ip = c.get_destination_ip();

                // check if the desination is in our subnet
                if ipv4net.contains(&dst_ip) {
                    // search for the destination NIC
                    if let Some(nic) = ri.nics.get(&dst_ip.to_string()) {
                        // found the NIC, forward the chunk to the NIC.
                        // call to NIC must unlock mutex
                        let ni = nic.lock().await;
                        ni.on_inbound_chunk(c).await;
                    } else {
                        // NIC not found. drop it.
                        log::debug!("[{}] {} unreachable", name, c.to_string());
                    }
                    continue;
                }

                // the destination is outside of this subnet
                // is this WAN?
                if let Some(parent) = &ri.parent {
                    // Pass it to the parent via NAT
                    if let Some(to_parent) = ri.nat.translate_outbound(&*c).await? {
                        // call to parent router mutex unlock mutex
                        let p = parent.lock().await;
                        p.push(to_parent).await;
                    }
                } else {
                    // this WAN. No route for this chunk
                    log::debug!("[{}] no route found for {}", name, c.to_string());
                }
            } else {
                break; // no more chunk in the queue
            }
        }

        Ok(d)
    }
}

impl RouterInternal {
    // caller must hold the mutex
    pub(crate) async fn add_nic(
        &mut self,
        nic: Arc<Mutex<dyn NIC + Send + Sync>>,
    ) -> Result<(), Error> {
        let mut ips = {
            let ni = nic.lock().await;
            ni.get_static_ips().to_vec()
        };

        if ips.is_empty() {
            // assign an IP address
            let ip = self.assign_ip_address()?;
            ips.push(ip);
        }

        let mut ipnets = vec![];
        for ip in &ips {
            if !self.ipv4net.contains(ip) {
                return Err(ERR_STATIC_IP_IS_BEYOND_SUBNET.to_owned());
            }
            self.nics.insert(ip.to_string(), Arc::clone(&nic));
            ipnets.push(IpNet::from_str(&format!(
                "{}/{}",
                ip,
                self.ipv4net.prefix_len()
            ))?);
        }

        {
            let mut ni = nic.lock().await;
            let _ = ni.add_addrs_to_interface("eth0", &ipnets).await;
        }

        Ok(())
    }

    // caller should hold the mutex
    fn assign_ip_address(&mut self) -> Result<IpAddr, Error> {
        // See: https://stackoverflow.com/questions/14915188/ip-address-ending-with-zero

        if self.last_id == 0xfe {
            return Err(ERR_ADDRESS_SPACE_EXHAUSTED.to_owned());
        }

        self.last_id += 1;
        match self.ipv4net.addr() {
            IpAddr::V4(ipv4) => {
                let mut ip = ipv4.octets();
                ip[3] += 1;
                Ok(IpAddr::V4(Ipv4Addr::from(ip)))
            }
            IpAddr::V6(ipv6) => {
                let mut ip = ipv6.octets();
                ip[15] += 1;
                Ok(IpAddr::V6(Ipv6Addr::from(ip)))
            }
        }
    }
}
