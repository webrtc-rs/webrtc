use crate::error::Error;
use crate::mux::mux_func::MatchFunc;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;
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

    /// Read reads a packet of len(p) bytes from the underlying conn
    /// that are matched by the associated MuxFunc
    pub async fn read(
        &self,
        packet: &mut [u8],
        duration: Option<Duration>,
    ) -> Result<usize, Error> {
        let n = self.buffer.read(packet, duration).await?;
        Ok(n)
    }

    /// writes bytes to the underlying conn
    pub async fn write(&self, buf: &[u8]) -> std::io::Result<usize> {
        self.next_conn.send(buf).await
    }

    pub async fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.next_conn.local_addr().await
    }

    // RemoteAddr is a stub
    //func (e *Endpoint) RemoteAddr() net.Addr {
    //    return e.mux.next_conn.RemoteAddr()
    // }
}
