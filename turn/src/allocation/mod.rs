#[cfg(test)]
mod allocation_test;

pub mod allocation_manager;
pub mod channel_bind;
pub mod five_tuple;
pub mod permission;

use std::collections::HashMap;
use std::marker::{Send, Sync};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use channel_bind::*;
use five_tuple::*;
use permission::*;
use portable_atomic::{AtomicBool, AtomicUsize};
use stun::agent::*;
use stun::message::*;
use stun::textattrs::Username;
use tokio::sync::oneshot::{self, Sender};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{Duration, Instant};
use util::sync::Mutex as SyncMutex;
use util::Conn;

use crate::error::*;
use crate::proto::chandata::*;
use crate::proto::channum::*;
use crate::proto::data::*;
use crate::proto::peeraddr::*;
use crate::proto::*;

const RTP_MTU: usize = 1500;

pub type AllocationMap = Arc<Mutex<HashMap<FiveTuple, Arc<Allocation>>>>;

/// Information about an [`Allocation`].
#[derive(Debug, Clone)]
pub struct AllocationInfo {
    /// [`FiveTuple`] of this [`Allocation`].
    pub five_tuple: FiveTuple,

    /// Username of this [`Allocation`].
    pub username: String,

    /// Relayed bytes with this [`Allocation`].
    #[cfg(feature = "metrics")]
    pub relayed_bytes: usize,
}

impl AllocationInfo {
    /// Creates a new [`AllocationInfo`].
    pub fn new(
        five_tuple: FiveTuple,
        username: String,
        #[cfg(feature = "metrics")] relayed_bytes: usize,
    ) -> Self {
        Self {
            five_tuple,
            username,
            #[cfg(feature = "metrics")]
            relayed_bytes,
        }
    }
}

/// `Allocation` is tied to a FiveTuple and relays traffic
/// use create_allocation and get_allocation to operate.
pub struct Allocation {
    protocol: Protocol,
    turn_socket: Arc<dyn Conn + Send + Sync>,
    pub(crate) relay_addr: SocketAddr,
    pub(crate) relay_socket: Arc<dyn Conn + Send + Sync>,
    five_tuple: FiveTuple,
    username: Username,
    permissions: Arc<Mutex<HashMap<String, Permission>>>,
    channel_bindings: Arc<Mutex<HashMap<ChannelNumber, ChannelBind>>>,
    pub(crate) allocations: Option<AllocationMap>,
    reset_tx: SyncMutex<Option<mpsc::Sender<Duration>>>,
    timer_expired: Arc<AtomicBool>,
    closed: AtomicBool, // Option<mpsc::Receiver<()>>,
    pub(crate) relayed_bytes: AtomicUsize,
    drop_tx: Option<Sender<u32>>,
    alloc_close_notify: Option<mpsc::Sender<AllocationInfo>>,
}

fn addr2ipfingerprint(addr: &SocketAddr) -> String {
    addr.ip().to_string()
}

impl Allocation {
    /// Creates a new [`Allocation`].
    pub fn new(
        turn_socket: Arc<dyn Conn + Send + Sync>,
        relay_socket: Arc<dyn Conn + Send + Sync>,
        relay_addr: SocketAddr,
        five_tuple: FiveTuple,
        username: Username,
        alloc_close_notify: Option<mpsc::Sender<AllocationInfo>>,
    ) -> Self {
        Allocation {
            protocol: PROTO_UDP,
            turn_socket,
            relay_addr,
            relay_socket,
            five_tuple,
            username,
            permissions: Arc::new(Mutex::new(HashMap::new())),
            channel_bindings: Arc::new(Mutex::new(HashMap::new())),
            allocations: None,
            reset_tx: SyncMutex::new(None),
            timer_expired: Arc::new(AtomicBool::new(false)),
            closed: AtomicBool::new(false),
            relayed_bytes: Default::default(),
            drop_tx: None,
            alloc_close_notify,
        }
    }

    /// Checks the Permission for the `addr`.
    pub async fn has_permission(&self, addr: &SocketAddr) -> bool {
        let permissions = self.permissions.lock().await;
        permissions.get(&addr2ipfingerprint(addr)).is_some()
    }

    /// Adds a new [`Permission`] to this [`Allocation`].
    pub async fn add_permission(&self, mut p: Permission) {
        let fingerprint = addr2ipfingerprint(&p.addr);

        {
            let permissions = self.permissions.lock().await;
            if let Some(existed_permission) = permissions.get(&fingerprint) {
                existed_permission.refresh(PERMISSION_TIMEOUT).await;
                return;
            }
        }

        p.permissions = Some(Arc::clone(&self.permissions));
        p.start(PERMISSION_TIMEOUT).await;

        {
            let mut permissions = self.permissions.lock().await;
            permissions.insert(fingerprint, p);
        }
    }

    /// Removes the `addr`'s fingerprint from this [`Allocation`]'s permissions.
    pub async fn remove_permission(&self, addr: &SocketAddr) -> bool {
        let mut permissions = self.permissions.lock().await;
        permissions.remove(&addr2ipfingerprint(addr)).is_some()
    }

    /// Adds a new [`ChannelBind`] to this [`Allocation`], it also updates the
    /// permissions needed for this [`ChannelBind`].
    pub async fn add_channel_bind(&self, mut c: ChannelBind, lifetime: Duration) -> Result<()> {
        {
            if let Some(addr) = self.get_channel_addr(&c.number).await {
                if addr != c.peer {
                    return Err(Error::ErrSameChannelDifferentPeer);
                }
            }

            if let Some(number) = self.get_channel_number(&c.peer).await {
                if number != c.number {
                    return Err(Error::ErrSameChannelDifferentPeer);
                }
            }
        }

        {
            let channel_bindings = self.channel_bindings.lock().await;
            if let Some(cb) = channel_bindings.get(&c.number) {
                cb.refresh(lifetime).await;

                // Channel binds also refresh permissions.
                self.add_permission(Permission::new(cb.peer)).await;

                return Ok(());
            }
        }

        let peer = c.peer;

        // Add or refresh this channel.
        c.channel_bindings = Some(Arc::clone(&self.channel_bindings));
        c.start(lifetime).await;

        {
            let mut channel_bindings = self.channel_bindings.lock().await;
            channel_bindings.insert(c.number, c);
        }

        // Channel binds also refresh permissions.
        self.add_permission(Permission::new(peer)).await;

        Ok(())
    }

    /// Removes the [`ChannelBind`] from this [`Allocation`] by `number`.
    pub async fn remove_channel_bind(&self, number: ChannelNumber) -> bool {
        let mut channel_bindings = self.channel_bindings.lock().await;
        channel_bindings.remove(&number).is_some()
    }

    /// Gets the [`ChannelBind`]'s address by `number`.
    pub async fn get_channel_addr(&self, number: &ChannelNumber) -> Option<SocketAddr> {
        let channel_bindings = self.channel_bindings.lock().await;
        channel_bindings.get(number).map(|cb| cb.peer)
    }

    /// Gets the [`ChannelBind`]'s number from this [`Allocation`] by `addr`.
    pub async fn get_channel_number(&self, addr: &SocketAddr) -> Option<ChannelNumber> {
        let channel_bindings = self.channel_bindings.lock().await;
        for cb in channel_bindings.values() {
            if cb.peer == *addr {
                return Some(cb.number);
            }
        }
        None
    }

    /// Closes the [`Allocation`].
    pub async fn close(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            return Err(Error::ErrClosed);
        }

        self.closed.store(true, Ordering::Release);
        self.stop();

        {
            let mut permissions = self.permissions.lock().await;
            for p in permissions.values_mut() {
                p.stop();
            }
        }

        {
            let mut channel_bindings = self.channel_bindings.lock().await;
            for c in channel_bindings.values_mut() {
                c.stop();
            }
        }

        log::trace!("allocation with {} closed!", self.five_tuple);

        let _ = self.turn_socket.close().await;
        let _ = self.relay_socket.close().await;

        if let Some(notify_tx) = &self.alloc_close_notify {
            let _ = notify_tx
                .send(AllocationInfo {
                    five_tuple: self.five_tuple,
                    username: self.username.text.clone(),
                    #[cfg(feature = "metrics")]
                    relayed_bytes: self.relayed_bytes.load(Ordering::Acquire),
                })
                .await;
        }

        Ok(())
    }

    pub async fn start(&self, lifetime: Duration) {
        let (reset_tx, mut reset_rx) = mpsc::channel(1);
        self.reset_tx.lock().replace(reset_tx);

        let allocations = self.allocations.clone();
        let five_tuple = self.five_tuple;
        let timer_expired = Arc::clone(&self.timer_expired);

        tokio::spawn(async move {
            let timer = tokio::time::sleep(lifetime);
            tokio::pin!(timer);
            let mut done = false;

            while !done {
                tokio::select! {
                    _ = &mut timer => {
                        if let Some(allocs) = &allocations{
                            let mut allocs = allocs.lock().await;
                            if let Some(a) = allocs.remove(&five_tuple) {
                                let _ = a.close().await;
                            }
                        }
                        done = true;
                    },
                    result = reset_rx.recv() => {
                        if let Some(d) = result {
                            timer.as_mut().reset(Instant::now() + d);
                        } else {
                            done = true;
                        }
                    },
                }
            }

            timer_expired.store(true, Ordering::SeqCst);
        });
    }

    fn stop(&self) -> bool {
        let reset_tx = self.reset_tx.lock().take();
        reset_tx.is_none() || self.timer_expired.load(Ordering::SeqCst)
    }

    /// Updates the allocations lifetime.
    pub async fn refresh(&self, lifetime: Duration) {
        let reset_tx = self.reset_tx.lock().clone();
        if let Some(tx) = reset_tx {
            let _ = tx.send(lifetime).await;
        }
    }

    //  https://tools.ietf.org/html/rfc5766#section-10.3
    //  When the server receives a UDP datagram at a currently allocated
    //  relayed transport address, the server looks up the allocation
    //  associated with the relayed transport address.  The server then
    //  checks to see whether the set of permissions for the allocation allow
    //  the relaying of the UDP datagram as described in Section 8.
    //
    //  If relaying is permitted, then the server checks if there is a
    //  channel bound to the peer that sent the UDP datagram (see
    //  Section 11).  If a channel is bound, then processing proceeds as
    //  described in Section 11.7.
    //
    //  If relaying is permitted but no channel is bound to the peer, then
    //  the server forms and sends a Data indication.  The Data indication
    //  MUST contain both an XOR-PEER-ADDRESS and a DATA attribute.  The DATA
    //  attribute is set to the value of the 'data octets' field from the
    //  datagram, and the XOR-PEER-ADDRESS attribute is set to the source
    //  transport address of the received UDP datagram.  The Data indication
    //  is then sent on the 5-tuple associated with the allocation.
    async fn packet_handler(&mut self) {
        let five_tuple = self.five_tuple;
        let relay_addr = self.relay_addr;
        let relay_socket = Arc::clone(&self.relay_socket);
        let turn_socket = Arc::clone(&self.turn_socket);
        let allocations = self.allocations.clone();
        let channel_bindings = Arc::clone(&self.channel_bindings);
        let permissions = Arc::clone(&self.permissions);
        let (drop_tx, drop_rx) = oneshot::channel::<u32>();
        self.drop_tx = Some(drop_tx);

        tokio::spawn(async move {
            let mut buffer = vec![0u8; RTP_MTU];

            tokio::pin!(drop_rx);

            loop {
                let (n, src_addr) = tokio::select! {
                    result = relay_socket.recv_from(&mut buffer) => {
                        match result {
                            Ok((n, src_addr)) => (n, src_addr),
                            Err(_) => {
                                if let Some(allocs) = &allocations {
                                    let mut allocs = allocs.lock().await;
                                    allocs.remove(&five_tuple);
                                }
                                break;
                            }
                        }
                    }
                    _ = drop_rx.as_mut() => {
                        log::trace!("allocation has stopped, stop packet_handler. five_tuple: {:?}", five_tuple);
                        break;
                    }
                };

                log::debug!(
                    "relay socket {:?} received {} bytes from {}",
                    relay_socket.local_addr(),
                    n,
                    src_addr
                );

                let cb_number = {
                    let mut cb_number = None;
                    let cbs = channel_bindings.lock().await;
                    for cb in cbs.values() {
                        if cb.peer == src_addr {
                            cb_number = Some(cb.number);
                            break;
                        }
                    }
                    cb_number
                };

                if let Some(number) = cb_number {
                    let mut channel_data = ChannelData {
                        data: buffer[..n].to_vec(),
                        number,
                        raw: vec![],
                    };
                    channel_data.encode();

                    if let Err(err) = turn_socket
                        .send_to(&channel_data.raw, five_tuple.src_addr)
                        .await
                    {
                        log::error!(
                            "Failed to send ChannelData from allocation {} {}",
                            src_addr,
                            err
                        );
                    }
                } else {
                    let exist = {
                        let ps = permissions.lock().await;
                        ps.get(&addr2ipfingerprint(&src_addr)).is_some()
                    };

                    if exist {
                        let msg = {
                            let peer_address_attr = PeerAddress {
                                ip: src_addr.ip(),
                                port: src_addr.port(),
                            };
                            let data_attr = Data(buffer[..n].to_vec());

                            let mut msg = Message::new();
                            if let Err(err) = msg.build(&[
                                Box::new(TransactionId::new()),
                                Box::new(MessageType::new(METHOD_DATA, CLASS_INDICATION)),
                                Box::new(peer_address_attr),
                                Box::new(data_attr),
                            ]) {
                                log::error!(
                                    "Failed to send DataIndication from allocation {} {}",
                                    src_addr,
                                    err
                                );
                                None
                            } else {
                                Some(msg)
                            }
                        };

                        if let Some(msg) = msg {
                            log::debug!(
                                "relaying message from {} to client at {}",
                                src_addr,
                                five_tuple.src_addr
                            );
                            if let Err(err) =
                                turn_socket.send_to(&msg.raw, five_tuple.src_addr).await
                            {
                                log::error!(
                                    "Failed to send DataIndication from allocation {} {}",
                                    src_addr,
                                    err
                                );
                            }
                        }
                    } else {
                        log::info!(
                            "No Permission or Channel exists for {} on allocation {}",
                            src_addr,
                            relay_addr
                        );
                    }
                }
            }
        });
    }
}
