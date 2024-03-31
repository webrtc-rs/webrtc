use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use util::{Buffer, Conn};

use crate::mux::mux_func::MatchFunc;

/// Endpoint implements net.Conn. It is used to read muxed packets.
pub struct Endpoint {
    pub(crate) id: usize,
    pub(crate) buffer: Buffer,
    pub(crate) match_fn: MatchFunc,
    pub(crate) next_conn: Arc<dyn Conn + Send + Sync>,
    pub(crate) endpoints: Arc<Mutex<HashMap<usize, Arc<Endpoint>>>>,
}

impl Endpoint {
    /// Close unregisters the endpoint from the Mux
    pub async fn close(&self) -> Result<()> {
        self.buffer.close().await;

        let mut endpoints = self.endpoints.lock().await;
        endpoints.remove(&self.id);

        Ok(())
    }
}

type Result<T> = std::result::Result<T, util::Error>;

#[async_trait]
impl Conn for Endpoint {
    async fn connect(&self, _addr: SocketAddr) -> Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    /// reads a packet of len(p) bytes from the underlying conn
    /// that are matched by the associated MuxFunc
    async fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        match self.buffer.read(buf, None).await {
            Ok(n) => Ok(n),
            Err(err) => Err(io::Error::new(io::ErrorKind::Other, err.to_string()).into()),
        }
    }
    async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    /// writes bytes to the underlying conn
    async fn send(&self, buf: &[u8]) -> Result<usize> {
        self.next_conn.send(buf).await
    }

    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> Result<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        self.next_conn.local_addr()
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        self.next_conn.remote_addr()
    }

    async fn close(&self) -> Result<()> {
        self.next_conn.close().await
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}
