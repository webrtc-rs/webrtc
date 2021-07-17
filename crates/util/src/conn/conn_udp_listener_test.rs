use super::conn_udp_listener::*;
use super::error::Error;
use super::*;
use tokio::net::UdpSocket;

async fn pipe() -> Result<(
    Arc<dyn Listener + Send + Sync>,
    Arc<dyn Conn + Send + Sync>,
    UdpSocket,
)> {
    // Start listening
    let listener = Arc::new(listen("0.0.0.0:0").await?);

    // Open a connection
    let d_conn = UdpSocket::bind("0.0.0.0:0").await?;
    d_conn.connect(listener.addr().await?).await?;

    // Write to the connection to initiate it
    let handshake = "hello";
    d_conn.send(handshake.as_bytes()).await?;

    // Accept the connection
    let l_conn = listener.accept().await?;

    let mut buf = vec![0u8; handshake.len()];
    let n = l_conn.recv(&mut buf).await?;

    let result = String::from_utf8(buf[..n].to_vec())?;
    if handshake != result {
        Err(Error::new(format!("errHandshakeFailed: {} != {}", handshake, result)).into())
    } else {
        Ok((listener, l_conn, d_conn))
    }
}
