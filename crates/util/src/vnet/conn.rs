use super::errors::*;
use crate::conn::Conn;
use crate::vnet::chunk::{Chunk, ChunkUDP};
use crate::Error;

use std::io;
use std::net::{IpAddr, SocketAddr};
use tokio::sync::{mpsc, Mutex};

use async_trait::async_trait;
use std::sync::Arc;

const MAX_READ_QUEUE_SIZE: usize = 1024;

// vNet implements this
#[async_trait]
pub(crate) trait ConnObserver {
    async fn write(&self, c: Box<dyn Chunk + Send>) -> Result<(), Error>;
    //onClosed(addr net.Addr)
    fn determine_source_ip(&self, loc_ip: IpAddr, dst_ip: IpAddr) -> Option<IpAddr>;
}

// UDPConn is the implementation of the Conn and PacketConn interfaces for UDP network connections.
// comatible with net.PacketConn and net.Conn
pub(crate) struct UDPConn {
    loc_addr: SocketAddr,
    rem_addr: Option<SocketAddr>,
    read_ch: Mutex<mpsc::Receiver<Box<dyn Chunk + Send>>>,
    obs: Arc<Mutex<Box<dyn ConnObserver + Send + Sync>>>,
}

impl UDPConn {
    pub(crate) fn new(
        loc_addr: SocketAddr,
        rem_addr: Option<SocketAddr>,
        read_ch: mpsc::Receiver<Box<dyn Chunk + Send>>,
        obs: Arc<Mutex<Box<dyn ConnObserver + Send + Sync>>>,
    ) -> Self {
        UDPConn {
            loc_addr,
            rem_addr,
            read_ch: Mutex::new(read_ch),
            obs,
        }
    }
}

#[async_trait]
impl Conn for UDPConn {
    async fn connect(&self, _addr: SocketAddr) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }
    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let (n, _) = self.recv_from(buf).await?;
        Ok(n)
    }

    // recv_from reads a packet from the connection,
    // copying the payload into p. It returns the number of
    // bytes copied into p and the return address that
    // was on the packet.
    // It returns the number of bytes read (0 <= n <= len(p))
    // and any error encountered. Callers should always process
    // the n > 0 bytes returned before considering the error err.
    async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let mut read_ch = self.read_ch.lock().await;
        while let Some(chunk) = read_ch.recv().await {
            let user_data = chunk.user_data();
            let n = std::cmp::min(buf.len(), user_data.len());
            buf[..n].copy_from_slice(&user_data[..n]);
            let addr = chunk.source_addr();
            if let Some(rem_addr) = &self.rem_addr {
                if &addr != rem_addr {
                    continue; // discard (shouldn't happen)
                }
            }
            return Ok((n, addr));
        }

        Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "Connection Aborted",
        ))
    }

    async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        if let Some(rem_addr) = &self.rem_addr {
            self.send_to(buf, *rem_addr).await
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                ERR_NO_REM_ADDR.to_string(),
            ))
        }
    }

    // send_to writes a packet with payload p to addr.
    // send_to can be made to time out and return
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> io::Result<usize> {
        let src_ip = {
            let obs = self.obs.lock().await;
            match obs.determine_source_ip(self.loc_addr.ip(), target.ip()) {
                Some(ip) => ip,
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        ERR_LOC_ADDR.to_string(),
                    ))
                }
            }
        };

        let src_addr = SocketAddr::new(src_ip, self.loc_addr.port());

        let mut chunk = ChunkUDP::new(src_addr, target);
        chunk.user_data = buf.to_vec();
        let result = {
            let c: Box<dyn Chunk + Send> = Box::new(chunk);
            let obs = self.obs.lock().await;
            obs.write(c).await
        };
        if let Err(err) = result {
            return Err(io::Error::new(io::ErrorKind::Other, err.to_string()));
        }

        Ok(buf.len())
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.loc_addr)
    }
}
