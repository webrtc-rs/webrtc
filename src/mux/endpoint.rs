use crate::error::Error;
use crate::mux::mux_func::MatchFunc;
use async_trait::async_trait;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use util::{Buffer, Conn};

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
    pub async fn close(&self) -> Result<(), Error> {
        self.buffer.close().await;

        let mut endpoints = self.endpoints.lock().await;
        endpoints.remove(&self.id);

        Ok(())
    }
}

#[async_trait]
impl Conn for Endpoint {
    async fn connect(&self, _addr: SocketAddr) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }

    /// reads a packet of len(p) bytes from the underlying conn
    /// that are matched by the associated MuxFunc
    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        match self.buffer.read(buf, None).await {
            Ok(n) => Ok(n),
            Err(err) => Err(io::Error::new(io::ErrorKind::Other, err.to_string())),
        }
    }
    async fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }

    /// writes bytes to the underlying conn
    async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.next_conn.send(buf).await
    }

    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }
    async fn local_addr(&self) -> io::Result<SocketAddr> {
        self.next_conn.local_addr().await
    }
}
