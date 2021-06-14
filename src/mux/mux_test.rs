use super::*;
use crate::mux::mux_func::match_all;
use async_trait::async_trait;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use util::conn::conn_pipe::pipe;

const TEST_PIPE_BUFFER_SIZE: usize = 8192;

async fn pipe_memory() -> (Arc<Endpoint>, impl Conn) {
    // In memory pipe
    let (ca, cb) = pipe();

    let mut m = Mux::new(Config {
        conn: Arc::new(ca),
        buffer_size: TEST_PIPE_BUFFER_SIZE,
    });

    let e = m.new_endpoint(Box::new(match_all)).await;
    m.remove_endpoint(&e).await;
    let e = m.new_endpoint(Box::new(match_all)).await;

    (e, cb)
}

#[tokio::test]
async fn test_no_endpoints() -> Result<(), Error> {
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

#[async_trait]
impl Conn for MuxErrorConn {
    async fn connect(&self, _addr: SocketAddr) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }

    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let idx = self.idx.fetch_add(1, Ordering::SeqCst);
        if idx < self.data.len() {
            let n = std::cmp::min(buf.len(), self.data[idx].len());
            buf[..n].copy_from_slice(&self.data[idx][..n]);
            Ok(n)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("idx {} >= data.len {}", idx, self.data.len()),
            ))
        }
    }

    async fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }

    async fn send(&self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }

    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }

    async fn local_addr(&self) -> io::Result<SocketAddr> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable"))
    }
}

#[tokio::test]
async fn test_non_fatal_read() -> Result<(), Error> {
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

    let n = e.read(&mut buff, None).await?;
    assert_eq!(&buff[..n], expected_data);

    let n = e.read(&mut buff, None).await?;
    assert_eq!(&buff[..n], expected_data);

    let n = e.read(&mut buff, None).await?;
    assert_eq!(&buff[..n], expected_data);

    m.close().await;

    Ok(())
}
