use std::future::Future;
use std::io::BufReader;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::net::ToSocketAddrs;
use util::conn::conn_udp_listener::*;
use util::conn::*;

use crate::config::*;
use crate::conn::DTLSConn;
use crate::content::ContentType;
use crate::error::Result;
use crate::record_layer::record_layer_header::RecordLayerHeader;
use crate::record_layer::unpack_datagram;

/// Listen creates a DTLS listener
pub async fn listen<A: 'static + ToSocketAddrs>(laddr: A, config: Config) -> Result<impl Listener> {
    validate_config(false, &config)?;

    let mut lc = ListenConfig {
        accept_filter: Some(Box::new(
            |packet: &[u8]| -> Pin<Box<dyn Future<Output = bool> + Send + 'static>> {
                let pkts = match unpack_datagram(packet) {
                    Ok(pkts) => {
                        if pkts.is_empty() {
                            return Box::pin(async { false });
                        }
                        pkts
                    }
                    Err(_) => return Box::pin(async { false }),
                };

                let mut reader = BufReader::new(pkts[0].as_slice());
                match RecordLayerHeader::unmarshal(&mut reader) {
                    Ok(h) => {
                        let content_type = h.content_type;
                        Box::pin(async move { content_type == ContentType::Handshake })
                    }
                    Err(_) => Box::pin(async { false }),
                }
            },
        )),
        ..Default::default()
    };

    let parent = Arc::new(lc.listen(laddr).await?);
    Ok(DTLSListener { parent, config })
}

/// DTLSListener represents a DTLS listener
pub struct DTLSListener {
    parent: Arc<dyn Listener + Send + Sync>,
    config: Config,
}

impl DTLSListener {
    ///  creates a DTLS listener which accepts connections from an inner Listener.
    pub fn new(parent: Arc<dyn Listener + Send + Sync>, config: Config) -> Result<Self> {
        validate_config(false, &config)?;

        Ok(DTLSListener { parent, config })
    }
}

type UtilResult<T> = std::result::Result<T, util::Error>;

#[async_trait]
impl Listener for DTLSListener {
    /// Accept waits for and returns the next connection to the listener.
    /// You have to either close or read on all connection that are created.
    /// Connection handshake will timeout using ConnectContextMaker in the Config.
    /// If you want to specify the timeout duration, set ConnectContextMaker.
    async fn accept(&self) -> UtilResult<(Arc<dyn Conn + Send + Sync>, SocketAddr)> {
        let (conn, raddr) = self.parent.accept().await?;
        let dtls_conn = DTLSConn::new(conn, self.config.clone(), false, None)
            .await
            .map_err(util::Error::from_std)?;
        Ok((Arc::new(dtls_conn), raddr))
    }

    /// Close closes the listener.
    /// Any blocked Accept operations will be unblocked and return errors.
    /// Already Accepted connections are not closed.
    async fn close(&self) -> UtilResult<()> {
        self.parent.close().await
    }

    /// Addr returns the listener's network address.
    async fn addr(&self) -> UtilResult<SocketAddr> {
        self.parent.addr().await
    }
}
