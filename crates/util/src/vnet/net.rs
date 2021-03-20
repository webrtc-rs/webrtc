#[cfg(test)]
mod net_test;

use super::conn_map::*;
use super::errors::*;
use super::interface::*;
use crate::vnet::chunk::Chunk;
use crate::vnet::conn::ConnObserver;
use crate::vnet::router::*;
use crate::Error;

use async_trait::async_trait;
use ipnet::IpNet;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) const LO0_STR: &str = "lo0";
pub(crate) const UDP_STR: &str = "udp";

lazy_static! {
    pub static ref MAC_ADDR_COUNTER: AtomicU64 = AtomicU64::new(0xBEEFED910200);
}

pub(crate) type HardwareAddr = Vec<u8>;

pub(crate) fn new_mac_address() -> HardwareAddr {
    let b = MAC_ADDR_COUNTER
        .fetch_add(1, Ordering::SeqCst)
        .to_be_bytes();
    b[2..].to_vec()
}

pub struct VNet {
    interfaces: Vec<Interface>,                // read-only
    static_ips: Vec<IpAddr>,                   // read-only
    router: Mutex<Option<Arc<Mutex<Router>>>>, // read-only
    udp_conns: UDPConnMap,                     // read-only
}

#[async_trait]
impl NIC for VNet {
    fn get_interface(&self, ifc_name: &str) -> Option<&Interface> {
        for ifc in &self.interfaces {
            if ifc.name == ifc_name {
                return Some(ifc);
            }
        }
        None
    }

    fn get_interface_mut(&mut self, ifc_name: &str) -> Option<&mut Interface> {
        for ifc in &mut self.interfaces {
            if ifc.name == ifc_name {
                return Some(ifc);
            }
        }
        None
    }

    async fn set_router(&self, r: Arc<Mutex<Router>>) -> Result<(), Error> {
        let mut router = self.router.lock().await;
        *router = Some(r);

        Ok(())
    }

    async fn on_inbound_chunk(&self, c: Box<dyn Chunk + Send + Sync>) {
        if c.network() == UDP_STR {
            if let Some(conn) = self.udp_conns.find(&c.destination_addr()).await {
                let tx = conn.get_inbound_ch();
                let _ = tx.send(c).await;
            }
        }
    }

    fn get_static_ips(&self) -> &[IpAddr] {
        &[]
    }
}

#[async_trait]
impl ConnObserver for VNet {
    async fn write(&self, c: Box<dyn Chunk + Send + Sync>) -> Result<(), Error> {
        if c.network() == UDP_STR && c.get_destination_ip().is_loopback() {
            if let Some(conn) = self.udp_conns.find(&c.destination_addr()).await {
                let tx = conn.get_inbound_ch();
                let _ = tx.send(c).await;
            }
            return Ok(());
        }

        let router = self.router.lock().await;
        if let Some(r) = &*router {
            let p = r.lock().await;
            p.push(c).await;
            Ok(())
        } else {
            Err(ERR_NO_ROUTER_LINKED.to_owned())
        }
    }

    // This method determines the srcIP based on the dstIP when locIP
    // is any IP address ("0.0.0.0" or "::"). If locIP is a non-any addr,
    // this method simply returns locIP.
    // caller must hold the mutex
    fn determine_source_ip(&self, loc_ip: IpAddr, dst_ip: IpAddr) -> Option<IpAddr> {
        if !loc_ip.is_unspecified() {
            return Some(loc_ip);
        }

        if dst_ip.is_loopback() {
            let src_ip = if let Ok(src_ip) = IpAddr::from_str("127.0.0.1") {
                Some(src_ip)
            } else {
                None
            };
            return src_ip;
        }

        if let Some(ifc) = self.get_interface("eth0") {
            for ipnet in ifc.addrs() {
                if (ipnet.addr().is_ipv4() && loc_ip.is_ipv4())
                    || (ipnet.addr().is_ipv6() && loc_ip.is_ipv6())
                {
                    return Some(ipnet.addr());
                }
            }
        }

        None
    }
}

impl VNet {
    pub(crate) fn get_interfaces(&self) -> &[Interface] {
        &self.interfaces
    }

    // caller must hold the mutex
    pub(crate) fn get_all_ipaddrs(&self, ipv6: bool) -> Vec<IpAddr> {
        let mut ips = vec![];

        for ifc in &self.interfaces {
            for ipnet in ifc.addrs() {
                if (ipv6 && ipnet.addr().is_ipv6()) || (!ipv6 && ipnet.addr().is_ipv4()) {
                    ips.push(ipnet.addr());
                }
            }
        }

        ips
    }

    // caller must hold the mutex
    pub(crate) fn has_ipaddr(&self, ip: IpAddr) -> bool {
        for ifc in &self.interfaces {
            for ipnet in ifc.addrs() {
                let loc_ip = ipnet.addr();

                match ip.to_string().as_str() {
                    "0.0.0.0" => {
                        if loc_ip.is_ipv4() {
                            return true;
                        }
                    }
                    "::" => {
                        if loc_ip.is_ipv6() {
                            return true;
                        }
                    }
                    _ => {
                        if loc_ip == ip {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    // caller must hold the mutex
    pub(crate) async fn allocate_local_addr(&self, ip: IpAddr, port: u16) -> Result<(), Error> {
        // gather local IP addresses to bind
        let mut ips = vec![];
        if ip.is_unspecified() {
            ips = self.get_all_ipaddrs(ip.is_ipv6());
        } else if self.has_ipaddr(ip) {
            ips.push(ip);
        }

        if ips.is_empty() {
            return Err(ERR_BIND_FAILER_FOR.to_owned());
        }

        // check if all these transport addresses are not in use
        for ip2 in ips {
            let addr = SocketAddr::new(ip2, port);
            if self.udp_conns.find(&addr).await.is_some() {
                return Err(ERR_ADDRESS_ALREADY_IN_USE.to_owned());
            }
        }

        Ok(())
    }

    // caller must hold the mutex
    pub(crate) async fn assign_port(&self, ip: IpAddr, start: u16, end: u16) -> Result<u16, Error> {
        // choose randomly from the range between start and end (inclusive)
        if end < start {
            return Err(ERR_END_PORT_LESS_THAN_START.to_owned());
        }

        let space = end + 1 - start;
        let offset = rand::random::<u16>() % space;
        for i in 0..space {
            let port = ((offset + i) % space) + start;
            let result = self.allocate_local_addr(ip, port).await;
            if result.is_ok() {
                return Ok(port);
            }
        }

        Err(ERR_PORT_SPACE_EXHAUSTED.to_owned())
    }

    pub(crate) async fn resolve_addr(&self, address: &str) -> Result<SocketAddr, Error> {
        let v: Vec<&str> = address.splitn(2, ':').collect();
        if v.len() != 2 {
            return Err(ERR_ADDR_NOT_UDPADDR.to_owned());
        }
        let (host, port) = (v[0], v[1]);

        // Check if host is a domain name
        let ip: IpAddr = match host.parse() {
            Ok(ip) => ip,
            Err(_) => {
                let host = host.to_lowercase();
                if host == "localhost" {
                    Ipv4Addr::new(127, 0, 0, 1).into()
                } else {
                    // host is a domain name. resolve IP address by the name
                    let router_opt = self.router.lock().await;
                    if let Some(router) = &*router_opt {
                        let r = router.lock().await;
                        let resolver = r.resolver.lock().await;
                        if let Some(ip) = resolver.lookup(host).await {
                            ip
                        } else {
                            return Err(ERR_NOT_FOUND.to_owned());
                        }
                    } else {
                        return Err(ERR_NO_ROUTER_LINKED.to_owned());
                    }
                }
            }
        };

        let port: u16 = port.parse()?;

        Ok(SocketAddr::new(ip, port))
    }
}
/*


// caller must hold the mutex
func (v *vNet) _dialUDP(network string, locAddr, remAddr *net.UDPAddr) (UDPPacketConn, error) {
    // validate network
    if network != udpString && network != "udp4" {
        return nil, fmt.Errorf("%w: %s", errUnexpectedNetwork, network)
    }

    if locAddr == nil {
        locAddr = &net.UDPAddr{
            IP: net.IPv4zero,
        }
    } else if locAddr.IP == nil {
        locAddr.IP = net.IPv4zero
    }

    // validate address. do we have that address?
    if !v.hasIPAddr(locAddr.IP) {
        return nil, &net.OpError{
            Op:   "listen",
            Net:  network,
            Addr: locAddr,
            Err:  fmt.Errorf("bind: %w", errCantAssignRequestedAddr),
        }
    }

    if locAddr.Port == 0 {
        // choose randomly from the range between 5000 and 5999
        port, err := v.assignPort(locAddr.IP, 5000, 5999)
        if err != nil {
            return nil, &net.OpError{
                Op:   "listen",
                Net:  network,
                Addr: locAddr,
                Err:  err,
            }
        }
        locAddr.Port = port
    } else if _, ok := v.udp_conns.find(locAddr); ok {
        return nil, &net.OpError{
            Op:   "listen",
            Net:  network,
            Addr: locAddr,
            Err:  fmt.Errorf("bind: %w", errAddressAlreadyInUse),
        }
    }

    conn, err := newUDPConn(locAddr, remAddr, v)
    if err != nil {
        return nil, err
    }

    err = v.udp_conns.insert(conn)
    if err != nil {
        return nil, err
    }

    return conn, nil
}

func (v *vNet) listenPacket(network string, address string) (UDPPacketConn, error) {
    v.mutex.Lock()
    defer v.mutex.Unlock()

    locAddr, err := v.resolveUDPAddr(network, address)
    if err != nil {
        return nil, err
    }

    return v._dialUDP(network, locAddr, nil)
}

func (v *vNet) listenUDP(network string, locAddr *net.UDPAddr) (UDPPacketConn, error) {
    v.mutex.Lock()
    defer v.mutex.Unlock()

    return v._dialUDP(network, locAddr, nil)
}

func (v *vNet) dialUDP(network string, locAddr, remAddr *net.UDPAddr) (UDPPacketConn, error) {
    v.mutex.Lock()
    defer v.mutex.Unlock()

    return v._dialUDP(network, locAddr, remAddr)
}

func (v *vNet) dial(network string, address string) (UDPPacketConn, error) {
    v.mutex.Lock()
    defer v.mutex.Unlock()

    remAddr, err := v.resolveUDPAddr(network, address)
    if err != nil {
        return nil, err
    }

    // Determine source address
    srcIP := v.determineSourceIP(nil, remAddr.IP)

    locAddr := &net.UDPAddr{IP: srcIP, Port: 0}

    return v._dialUDP(network, locAddr, remAddr)
}



func (v *vNet) onClosed(addr net.Addr) {
    if addr.Network() == udpString {
        //nolint:errcheck
        v.udp_conns.delete(addr) // #nosec
    }
}
*/

// NetConfig is a bag of configuration parameters passed to NewNet().
pub struct NetConfig {
    // static_ips is an array of static IP addresses to be assigned for this Net.
    // If no static IP address is given, the router will automatically assign
    // an IP address.
    static_ips: Vec<String>,

    // static_ip is deprecated. Use static_ips.
    static_ip: String,
}

// Net represents a local network stack euivalent to a set of layers from NIC
// up to the transport (UDP / TCP) layer.
pub enum Net {
    VNet(VNet),
    IFS(Vec<Interface>),
}

impl Net {
    // NewNet creates an instance of Net.
    // If config is nil, the virtual network is disabled. (uses corresponding
    // net.Xxxx() operations.
    // By design, it always have lo0 and eth0 interfaces.
    // The lo0 has the address 127.0.0.1 assigned by default.
    // IP address for eth0 will be assigned when this Net is added to a router.
    pub fn new(config: Option<NetConfig>) -> Self {
        if let Some(config) = config {
            let mut lo0 = Interface::new(LO0_STR.to_owned(), vec![]);
            if let Ok(ipnet) = Interface::convert(
                SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 0),
                Some(SocketAddr::new(Ipv4Addr::new(255, 0, 0, 0).into(), 0)),
            ) {
                lo0.add_addr(ipnet);
            }

            let eth0 = Interface::new("eth0".to_owned(), vec![]);

            let mut static_ips = vec![];
            for ip_str in &config.static_ips {
                if let Ok(ip) = IpAddr::from_str(ip_str) {
                    static_ips.push(ip);
                }
            }
            if !config.static_ip.is_empty() {
                if let Ok(ip) = IpAddr::from_str(&config.static_ip) {
                    static_ips.push(ip);
                }
            }

            let vnet = VNet {
                interfaces: vec![lo0, eth0],
                static_ips,
                router: Mutex::new(None),
                udp_conns: UDPConnMap::new(),
            };

            Net::VNet(vnet)
        } else {
            let interfaces = match ifaces::ifaces() {
                Ok(ifs) => ifs,
                Err(_) => vec![],
            };

            let mut m: HashMap<String, Vec<IpNet>> = HashMap::new();
            for iface in interfaces {
                if let Some(addrs) = m.get_mut(&iface.name) {
                    if let Some(addr) = iface.addr {
                        if let Ok(inet) = Interface::convert(addr, iface.mask) {
                            addrs.push(inet);
                        }
                    }
                } else if let Some(addr) = iface.addr {
                    if let Ok(inet) = Interface::convert(addr, iface.mask) {
                        m.insert(iface.name, vec![inet]);
                    }
                }
            }

            let mut ifs = vec![];
            for (name, addrs) in m.into_iter() {
                ifs.push(Interface::new(name, addrs));
            }

            Net::IFS(ifs)
        }
    }

    // Interfaces returns a list of the system's network interfaces.
    pub fn get_interfaces(&self) -> &[Interface] {
        match self {
            Net::VNet(vnet) => &vnet.interfaces,
            Net::IFS(ifs) => &ifs,
        }
    }

    // IsVirtual tests if the virtual network is enabled.
    pub fn is_virtual(&self) -> bool {
        match self {
            Net::VNet(_) => true,
            Net::IFS(_) => false,
        }
    }
}

#[async_trait]
impl NIC for Net {
    fn get_interface(&self, ifc_name: &str) -> Option<&Interface> {
        match self {
            Net::VNet(vnet) => vnet.get_interface(ifc_name),
            Net::IFS(ifs) => {
                for ifc in ifs {
                    if ifc.name == ifc_name {
                        return Some(ifc);
                    }
                }
                None
            }
        }
    }

    fn get_interface_mut(&mut self, ifc_name: &str) -> Option<&mut Interface> {
        match self {
            Net::VNet(vnet) => vnet.get_interface_mut(ifc_name),
            Net::IFS(ifs) => {
                for ifc in ifs {
                    if ifc.name == ifc_name {
                        return Some(ifc);
                    }
                }
                None
            }
        }
    }

    async fn set_router(&self, r: Arc<Mutex<Router>>) -> Result<(), Error> {
        match self {
            Net::VNet(vnet) => vnet.set_router(r).await,
            Net::IFS(_) => Err(ERR_VNET_DISABLED.to_owned()),
        }
    }

    async fn on_inbound_chunk(&self, c: Box<dyn Chunk + Send + Sync>) {
        match self {
            Net::VNet(vnet) => vnet.on_inbound_chunk(c).await,
            Net::IFS(_) => {}
        }
    }

    fn get_static_ips(&self) -> &[IpAddr] {
        match self {
            Net::VNet(vnet) => vnet.get_static_ips(),
            Net::IFS(_) => &[],
        }
    }
}

/*
TODO: revisit Net APIs
// ListenPacket announces on the local network address.
func (n *Net) ListenPacket(network string, address string) (net.PacketConn, error) {
    if n.v == nil {
        return net.ListenPacket(network, address)
    }

    return n.v.listenPacket(network, address)
}

// ListenUDP acts like ListenPacket for UDP networks.
func (n *Net) ListenUDP(network string, locAddr *net.UDPAddr) (UDPPacketConn, error) {
    if n.v == nil {
        return net.ListenUDP(network, locAddr)
    }

    return n.v.listenUDP(network, locAddr)
}

// Dial connects to the address on the named network.
func (n *Net) Dial(network, address string) (net.Conn, error) {
    if n.v == nil {
        return net.Dial(network, address)
    }

    return n.v.dial(network, address)
}

// CreateDialer creates an instance of vnet.Dialer
func (n *Net) CreateDialer(dialer *net.Dialer) Dialer {
    if n.v == nil {
        return &vDialer{
            dialer: dialer,
        }
    }

    return &vDialer{
        dialer: dialer,
        v:      n.v,
    }
}

// DialUDP acts like Dial for UDP networks.
func (n *Net) DialUDP(network string, laddr, raddr *net.UDPAddr) (UDPPacketConn, error) {
    if n.v == nil {
        return net.DialUDP(network, laddr, raddr)
    }

    return n.v.dialUDP(network, laddr, raddr)
}

// ResolveUDPAddr returns an address of UDP end point.
func (n *Net) ResolveUDPAddr(network, address string) (*net.UDPAddr, error) {
    if n.v == nil {
        return net.ResolveUDPAddr(network, address)
    }

    return n.v.resolveUDPAddr(network, address)
}

// Dialer is identical to net.Dialer excepts that its methods
// (Dial, DialContext) are overridden to use virtual network.
// Use vnet.CreateDialer() to create an instance of this Dialer.
type Dialer interface {
    Dial(network, address string) (net.Conn, error)
}

type vDialer struct {
    dialer *net.Dialer
    v      *vNet
}

func (d *vDialer) Dial(network, address string) (net.Conn, error) {
    if d.v == nil {
        return d.dialer.Dial(network, address)
    }

    return d.v.dial(network, address)
}
*/
