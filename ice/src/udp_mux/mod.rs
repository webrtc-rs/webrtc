use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use tokio::sync::{watch, Mutex};
use util::sync::RwLock;
use util::{Conn, Error};

mod udp_mux_conn;
pub use udp_mux_conn::{UDPMuxConn, UDPMuxConnParams, UDPMuxWriter};

#[cfg(test)]
mod udp_mux_test;

mod socket_addr_ext;

use stun::attributes::ATTR_USERNAME;
use stun::message::{is_message as is_stun_message, Message as STUNMessage};

use crate::candidate::RECEIVE_MTU;

/// Normalize a target socket addr for sending over a given local socket addr. This is useful when
/// a dual stack socket is used, in which case an IPv4 target needs to be mapped to an IPv6
/// address.
fn normalize_socket_addr(target: &SocketAddr, socket_addr: &SocketAddr) -> SocketAddr {
    match (target, socket_addr) {
        (SocketAddr::V4(target_ipv4), SocketAddr::V6(_)) => {
            let ipv6_mapped = target_ipv4.ip().to_ipv6_mapped();

            SocketAddr::new(std::net::IpAddr::V6(ipv6_mapped), target_ipv4.port())
        }
        // This will fail later if target is IPv6 and socket is IPv4, we ignore it here
        (_, _) => *target,
    }
}

#[async_trait]
pub trait UDPMux {
    /// Close the muxing.
    async fn close(&self) -> Result<(), Error>;

    /// Get the underlying connection for a given ufrag.
    async fn get_conn(self: Arc<Self>, ufrag: &str) -> Result<Arc<dyn Conn + Send + Sync>, Error>;

    /// Remove the underlying connection for a given ufrag.
    async fn remove_conn_by_ufrag(&self, ufrag: &str);
}

pub struct UDPMuxParams {
    conn: Box<dyn Conn + Send + Sync>,
}

impl UDPMuxParams {
    pub fn new<C>(conn: C) -> Self
    where
        C: Conn + Send + Sync + 'static,
    {
        Self {
            conn: Box::new(conn),
        }
    }
}

pub struct UDPMuxDefault {
    /// The params this instance is configured with.
    /// Contains the underlying UDP socket in use
    params: UDPMuxParams,

    /// Maps from ufrag to the underlying connection.
    conns: Mutex<HashMap<String, UDPMuxConn>>,

    /// Maps from ip address to the underlying connection.
    address_map: RwLock<HashMap<SocketAddr, UDPMuxConn>>,

    // Close sender
    closed_watch_tx: Mutex<Option<watch::Sender<()>>>,

    /// Close receiver
    closed_watch_rx: watch::Receiver<()>,
}

impl UDPMuxDefault {
    pub fn new(params: UDPMuxParams) -> Arc<Self> {
        let (closed_watch_tx, closed_watch_rx) = watch::channel(());

        let mux = Arc::new(Self {
            params,
            conns: Mutex::default(),
            address_map: RwLock::default(),
            closed_watch_tx: Mutex::new(Some(closed_watch_tx)),
            closed_watch_rx: closed_watch_rx.clone(),
        });

        let cloned_mux = Arc::clone(&mux);
        cloned_mux.start_conn_worker(closed_watch_rx);

        mux
    }

    pub async fn is_closed(&self) -> bool {
        self.closed_watch_tx.lock().await.is_none()
    }

    /// Create a muxed connection for a given ufrag.
    fn create_muxed_conn(self: &Arc<Self>, ufrag: &str) -> Result<UDPMuxConn, Error> {
        let local_addr = self.params.conn.local_addr()?;

        let params = UDPMuxConnParams {
            local_addr,
            key: ufrag.into(),
            udp_mux: Arc::downgrade(self) as Weak<dyn UDPMuxWriter + Send + Sync>,
        };

        Ok(UDPMuxConn::new(params))
    }

    async fn conn_from_stun_message(&self, buffer: &[u8], addr: &SocketAddr) -> Option<UDPMuxConn> {
        let (result, message) = {
            let mut m = STUNMessage::new();

            (m.unmarshal_binary(buffer), m)
        };

        match result {
            Err(err) => {
                log::warn!("Failed to handle decode ICE from {}: {}", addr, err);
                None
            }
            Ok(_) => {
                let (attr, found) = message.attributes.get(ATTR_USERNAME);
                if !found {
                    log::warn!("No username attribute in STUN message from {}", &addr);
                    return None;
                }

                let s = match String::from_utf8(attr.value) {
                    // Per the RFC this shouldn't happen
                    // https://datatracker.ietf.org/doc/html/rfc5389#section-15.3
                    Err(err) => {
                        log::warn!(
                            "Failed to decode USERNAME from STUN message as UTF-8: {}",
                            err
                        );
                        return None;
                    }
                    Ok(s) => s,
                };

                let conns = self.conns.lock().await;
                let conn = s
                    .split(':')
                    .next()
                    .and_then(|ufrag| conns.get(ufrag))
                    .cloned();

                conn
            }
        }
    }

    fn start_conn_worker(self: Arc<Self>, mut closed_watch_rx: watch::Receiver<()>) {
        tokio::spawn(async move {
            let mut buffer = [0u8; RECEIVE_MTU];

            loop {
                let loop_self = Arc::clone(&self);
                let conn = &loop_self.params.conn;

                tokio::select! {
                    res = conn.recv_from(&mut buffer) => {
                        match res {
                            Ok((len, addr)) => {
                                // Find connection based on previously having seen this source address
                                let conn = {
                                    let address_map = loop_self
                                        .address_map
                                        .read();

                                    address_map.get(&addr).cloned()
                                };

                                let conn = match conn {
                                    // If we couldn't find the connection based on source address, see if
                                    // this is a STUN message and if so if we can find the connection based on ufrag.
                                    None if is_stun_message(&buffer) => {
                                        loop_self.conn_from_stun_message(&buffer, &addr).await
                                    }
                                    s @ Some(_) => s,
                                    _ => None,
                                };

                                match conn {
                                    None => {
                                        log::trace!("Dropping packet from {}", &addr);
                                    }
                                    Some(conn) => {
                                        if let Err(err) = conn.write_packet(&buffer[..len], addr).await {
                                            log::error!("Failed to write packet: {}", err);
                                        }
                                    }
                                }
                            }
                            Err(Error::Io(err)) if err.0.kind() == ErrorKind::TimedOut => continue,
                            Err(err) => {
                                log::error!("Could not read udp packet: {}", err);
                                break;
                            }
                        }
                    }
                    _ = closed_watch_rx.changed() => {
                        return;
                    }
                }
            }
        });
    }
}

#[async_trait]
impl UDPMux for UDPMuxDefault {
    async fn close(&self) -> Result<(), Error> {
        if self.is_closed().await {
            return Err(Error::ErrAlreadyClosed);
        }

        let mut closed_tx = self.closed_watch_tx.lock().await;

        if let Some(tx) = closed_tx.take() {
            let _ = tx.send(());
            drop(closed_tx);

            let old_conns = {
                let mut conns = self.conns.lock().await;

                std::mem::take(&mut (*conns))
            };

            // NOTE: We don't wait for these closure to complete
            for (_, conn) in old_conns {
                conn.close();
            }

            {
                let mut address_map = self.address_map.write();

                // NOTE: This is important, we need to drop all instances of `UDPMuxConn` to
                // avoid a retain cycle due to the use of [`std::sync::Arc`] on both sides.
                let _ = std::mem::take(&mut (*address_map));
            }
        }

        Ok(())
    }

    async fn get_conn(self: Arc<Self>, ufrag: &str) -> Result<Arc<dyn Conn + Send + Sync>, Error> {
        if self.is_closed().await {
            return Err(Error::ErrUseClosedNetworkConn);
        }

        {
            let mut conns = self.conns.lock().await;
            if let Some(conn) = conns.get(ufrag) {
                // UDPMuxConn uses `Arc` internally so it's cheap to clone, but because
                // we implement `Conn` we need to further wrap it in an `Arc` here.
                return Ok(Arc::new(conn.clone()) as Arc<dyn Conn + Send + Sync>);
            }

            let muxed_conn = self.create_muxed_conn(ufrag)?;
            let mut close_rx = muxed_conn.close_rx();
            let cloned_self = Arc::clone(&self);
            let cloned_ufrag = ufrag.to_string();
            tokio::spawn(async move {
                let _ = close_rx.changed().await;

                // Arc needed
                cloned_self.remove_conn_by_ufrag(&cloned_ufrag).await;
            });

            conns.insert(ufrag.into(), muxed_conn.clone());

            Ok(Arc::new(muxed_conn) as Arc<dyn Conn + Send + Sync>)
        }
    }

    async fn remove_conn_by_ufrag(&self, ufrag: &str) {
        // Pion's ice implementation has both `RemoveConnByFrag` and `RemoveConn`, but since `conns`
        // is keyed on `ufrag` their implementation is equivalent.

        let removed_conn = {
            let mut conns = self.conns.lock().await;
            conns.remove(ufrag)
        };

        if let Some(conn) = removed_conn {
            let mut address_map = self.address_map.write();

            for address in conn.get_addresses() {
                address_map.remove(&address);
            }
        }
    }
}

#[async_trait]
impl UDPMuxWriter for UDPMuxDefault {
    async fn register_conn_for_address(&self, conn: &UDPMuxConn, addr: SocketAddr) {
        if self.is_closed().await {
            return;
        }

        let key = conn.key();
        {
            let mut addresses = self.address_map.write();

            addresses
                .entry(addr)
                .and_modify(|e| {
                    if e.key() != key {
                        e.remove_address(&addr);
                        *e = conn.clone();
                    }
                })
                .or_insert_with(|| conn.clone());
        }

        log::debug!("Registered {} for {}", addr, key);
    }

    async fn send_to(&self, buf: &[u8], target: &SocketAddr) -> Result<usize, Error> {
        self.params
            .conn
            .send_to(buf, *target)
            .await
            .map_err(Into::into)
    }
}
