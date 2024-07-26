use std::io;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use portable_atomic::AtomicUsize;
use util::conn::conn_pipe::pipe;

use super::*;
use crate::mux::mux_func::{match_all, match_srtp};

const TEST_PIPE_BUFFER_SIZE: usize = 8192;

#[tokio::test]
async fn test_no_endpoints() -> crate::error::Result<()> {
    // In memory pipe
    let (ca, _) = pipe();

    let mut m = Mux::new(Config {
        conn: Arc::new(ca),
        buffer_size: TEST_PIPE_BUFFER_SIZE,
    });

    Mux::dispatch(&[0], &m.endpoints).await?;
    m.close().await;

    Ok(())
}

struct MuxErrorConn {
    idx: AtomicUsize,
    data: Vec<Vec<u8>>,
}

type Result<T> = std::result::Result<T, util::Error>;

#[async_trait]
impl Conn for MuxErrorConn {
    async fn connect(&self, _addr: SocketAddr) -> Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    async fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        let idx = self.idx.fetch_add(1, Ordering::SeqCst);
        if idx < self.data.len() {
            let n = std::cmp::min(buf.len(), self.data[idx].len());
            buf[..n].copy_from_slice(&self.data[idx][..n]);
            Ok(n)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("idx {} >= data.len {}", idx, self.data.len()),
            )
            .into())
        }
    }

    async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    async fn send(&self, _buf: &[u8]) -> Result<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> Result<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        None
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

#[tokio::test]
async fn test_non_fatal_read() -> Result<()> {
    let expected_data = b"expected_data".to_vec();

    let conn = Arc::new(MuxErrorConn {
        idx: AtomicUsize::new(0),
        data: vec![
            expected_data.clone(),
            expected_data.clone(),
            expected_data.clone(),
        ],
    });

    let mut m = Mux::new(Config {
        conn,
        buffer_size: TEST_PIPE_BUFFER_SIZE,
    });

    let e = m.new_endpoint(Box::new(match_all)).await;
    let mut buff = vec![0u8; TEST_PIPE_BUFFER_SIZE];

    let n = e.recv(&mut buff).await?;
    assert_eq!(&buff[..n], expected_data);

    let n = e.recv(&mut buff).await?;
    assert_eq!(&buff[..n], expected_data);

    let n = e.recv(&mut buff).await?;
    assert_eq!(&buff[..n], expected_data);

    m.close().await;

    Ok(())
}

#[tokio::test]
async fn test_non_fatal_dispatch() -> Result<()> {
    let (ca, cb) = pipe();

    let mut m = Mux::new(Config {
        conn: Arc::new(ca),
        buffer_size: TEST_PIPE_BUFFER_SIZE,
    });

    let e = m.new_endpoint(Box::new(match_srtp)).await;
    e.buffer.set_limit_size(1).await;

    for _ in 0..25 {
        let srtp_packet = [128, 1, 2, 3, 4].to_vec();
        cb.send(&srtp_packet).await?;
    }

    m.close().await;

    Ok(())
}
