#[cfg(test)]
mod conn_map_test;

use super::errors::*;
use crate::vnet::conn::UDPConn;
use crate::{Conn, Error};

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

type PortMap = Mutex<HashMap<u16, Vec<Arc<UDPConn>>>>;

#[derive(Default)]
pub(crate) struct UDPConnMap {
    port_map: PortMap,
}

impl UDPConnMap {
    pub(crate) fn new() -> Self {
        UDPConnMap {
            port_map: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn insert(&self, conn: Arc<UDPConn>) -> Result<(), Error> {
        let addr = conn.local_addr().await?;

        let mut port_map = self.port_map.lock().await;
        if let Some(conns) = port_map.get(&addr.port()) {
            if addr.ip().is_unspecified() {
                return Err(ERR_ADDRESS_ALREADY_IN_USE.to_owned());
            }

            for c in conns {
                let laddr = c.local_addr().await?;
                if laddr.ip().is_unspecified() || laddr.ip() == addr.ip() {
                    return Err(ERR_ADDRESS_ALREADY_IN_USE.to_owned());
                }
            }
        }

        if let Some(conns) = port_map.get_mut(&addr.port()) {
            conns.push(conn);
        } else {
            port_map.insert(addr.port(), vec![conn]);
        }
        Ok(())
    }

    pub(crate) async fn find(&self, addr: &SocketAddr) -> Option<Arc<UDPConn>> {
        let port_map = self.port_map.lock().await;
        if let Some(conns) = port_map.get(&addr.port()) {
            if addr.ip().is_unspecified() {
                // pick the first one appears in the iteration
                if let Some(c) = conns.first() {
                    return Some(Arc::clone(c));
                } else {
                    return None;
                }
            }

            for c in conns {
                let laddr = {
                    match c.local_addr().await {
                        Ok(laddr) => laddr,
                        Err(_) => return None,
                    }
                };
                if laddr.ip().is_unspecified() || laddr.ip() == addr.ip() {
                    return Some(Arc::clone(c));
                }
            }
        }

        None
    }

    pub(crate) async fn delete(&self, addr: &SocketAddr) -> Result<(), Error> {
        let mut port_map = self.port_map.lock().await;
        let mut new_conns = vec![];
        if let Some(conns) = port_map.get(&addr.port()) {
            if !addr.ip().is_unspecified() {
                for c in conns {
                    let laddr = c.local_addr().await?;
                    if laddr.ip().is_unspecified() {
                        // This can't happen!
                        return Err(ERR_CANNOT_REMOVE_UNSPECIFIED_IP.to_owned());
                    }

                    if laddr.ip() == addr.ip() {
                        continue;
                    }
                    new_conns.push(Arc::clone(c));
                }
            }
        } else {
            return Err(ERR_NO_SUCH_UDPCONN.to_owned());
        }

        if new_conns.is_empty() {
            port_map.remove(&addr.port());
        } else {
            port_map.insert(addr.port(), new_conns);
        }

        Ok(())
    }

    pub(crate) async fn len(&self) -> usize {
        let port_map = self.port_map.lock().await;
        let mut n = 0;
        for conns in port_map.values() {
            n += conns.len();
        }
        n
    }
}
