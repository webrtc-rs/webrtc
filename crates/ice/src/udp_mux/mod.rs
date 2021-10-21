use std::{
    collections::HashMap,
    io::ErrorKind,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
};

use util::{Conn, Error};

use async_trait::async_trait;

use tokio::net::UdpSocket;
use tokio::sync::Mutex;

mod udp_mux_conn;
use udp_mux_conn::{UDPMuxConn, UDPMuxConnParams};

#[cfg(test)]
mod udp_mux_test;

mod socket_addr_ext;

use stun::{
    attributes::ATTR_USERNAME,
    message::{is_message as is_stun_message, Message as STUNMessage},
};

use crate::candidate::RECEIVE_MTU;

#[async_trait]
pub trait UDPMux {
    /// Close the muxing.
    async fn close(&self);

    /// Get the underlying connection for a given ufrag.
    async fn get_conn(self: Arc<Self>, ufrag: &str) -> Result<Arc<dyn Conn + Send + Sync>, Error>;

    /// Remove the underlying connection for a given ufrag.
    async fn remove_conn_by_frag(&self, ufrag: &str);
}

#[derive(Debug)]
pub struct UDPMuxParams {
    udp_socket: UdpSocket,
}

impl UDPMuxParams {
    pub fn new(udp_socket: UdpSocket) -> Self {
        Self { udp_socket }
    }
}

#[derive(Debug)]
pub struct UDPMuxDefault {
    /// The params this instance is configured with.
    /// Contains the underlying UDP socket in use
    params: UDPMuxParams,

    /// Maps from ufrag to the underlying connection.
    conns: Mutex<HashMap<String, UDPMuxConn>>,

    /// Maps from ip address to the underlying connection.
    address_map: RwLock<HashMap<SocketAddr, UDPMuxConn>>,

    /// Whether this connection has been closed
    closed: AtomicBool,
}

impl UDPMuxDefault {
    pub fn new(params: UDPMuxParams) -> Arc<Self> {
        let mux = Arc::new(Self {
            params,
            conns: Mutex::default(),
            address_map: RwLock::default(),
            closed: AtomicBool::new(false),
        });

        let cloned_mux = Arc::clone(&mux);
        cloned_mux.start_conn_worker();

        mux
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    async fn send_to(&self, buf: &[u8], target: &SocketAddr) -> Result<usize, Error> {
        self.params
            .udp_socket
            .send_to(buf, target)
            .await
            .map_err(Into::into)
    }

    /// Create a muxed connection for a given ufrag.
    async fn create_muxed_conn(self: &Arc<Self>, ufrag: &str) -> UDPMuxConn {
        let local_addr = self.params.udp_socket.local_addr().ok();

        let params = UDPMuxConnParams {
            local_addr,
            key: ufrag.into(),
            udp_mux: Arc::clone(self),
        };

        UDPMuxConn::new(params)
    }

    fn register_conn_for_address(&self, conn: &UDPMuxConn, addr: SocketAddr) {
        if self.is_closed() {
            return;
        }

        let key = conn.key();
        {
            let mut addresses = self
                .address_map
                .write()
                .expect("Failed to obtain write lock");

            addresses
                .entry(addr)
                .and_modify(|e| {
                    e.remove_address(&addr);
                    *e = conn.clone()
                })
                .or_insert_with(|| conn.clone());
        }

        log::debug!("Registered {} for {}", addr, key);
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
                    .map(Clone::clone);

                conn
            }
        }
    }

    fn start_conn_worker(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut buffer = [0u8; RECEIVE_MTU];

            loop {
                let loop_self = Arc::clone(&self);
                let socket = &loop_self.params.udp_socket;

                if loop_self.is_closed() {
                    break;
                }

                match socket.recv_from(&mut buffer).await {
                    Ok((len, addr)) => {
                        // Find connection based on previously having seen this source address
                        let conn = {
                            let address_map = loop_self
                                .address_map
                                .read()
                                .expect("Failed to acquire read lock");
                            address_map.get(&addr).map(Clone::clone)
                        };

                        let conn = match conn {
                            // If we couldn't find the connection based on source address, see if
                            // this is a STUN mesage and if so if we can find the connection based on ufrag.
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
                    Err(err) if err.kind() == ErrorKind::TimedOut => continue,
                    Err(err) => {
                        log::error!("Could not read udp packet: {}", err);
                        break;
                    }
                };
            }
        });
    }
}

#[async_trait]
impl UDPMux for UDPMuxDefault {
    async fn close(&self) {
        if self.is_closed() {
            return;
        }

        self.closed.store(true, Ordering::SeqCst);

        let old_conns = {
            let mut conns = self.conns.lock().await;

            std::mem::take(&mut (*conns))
        };

        // NOTE: We don't wait for these closure to complete
        for (_, conn) in old_conns.into_iter() {
            conn.close();
        }

        {
            let mut address_map = self
                .address_map
                .write()
                .expect("Failed to obtain address_map lock");

            // NOTE: This is important, we need to drop all instances of `UDPMuxConn` to
            // avoid a retain cycle due to the use of [`std::sync::Arc`] on both sides.
            let _ = std::mem::take(&mut (*address_map));
        }
    }

    async fn get_conn(self: Arc<Self>, ufrag: &str) -> Result<Arc<dyn Conn + Send + Sync>, Error> {
        if self.is_closed() {
            return Err(Error::ErrUseClosedNetworkConn);
        }

        {
            let mut conns = self.conns.lock().await;
            if let Some(conn) = conns.get(ufrag) {
                // UDPMuxConn uses `Arc` internally so it's cheap to clone, but because
                // we implement `Conn` we need to further wrap it in an `Arc` here.
                return Ok(Arc::new(conn.clone()) as Arc<dyn Conn + Send + Sync>);
            }

            let muxed_conn = self.create_muxed_conn(ufrag).await;
            let mut close_rx = muxed_conn.close_rx();
            let cloned_self = Arc::clone(&self);
            let cloned_ufrag = ufrag.to_string();
            tokio::spawn(async move {
                let _ = close_rx.changed().await;

                // Arc needed
                cloned_self.remove_conn_by_frag(&cloned_ufrag).await;
            });

            conns.insert(ufrag.into(), muxed_conn.clone());

            Ok(Arc::new(muxed_conn) as Arc<dyn Conn + Send + Sync>)
        }
    }

    async fn remove_conn_by_frag(&self, ufrag: &str) {
        // Pion's ice implementation has both `RemoveConnByFrag` and `RemoveConn`, but since `conns`
        // is keyed on `ufrag` their implementation is equivalent.

        let removed_conn = {
            let mut conns = self.conns.lock().await;
            conns.remove(ufrag)
        };

        if let Some(conn) = removed_conn {
            let mut address_map = self
                .address_map
                .write()
                .expect("Failed to obtain write lock for address_map");

            for address in conn.get_addresses() {
                address_map.remove(&address);
            }
        }
    }
}
