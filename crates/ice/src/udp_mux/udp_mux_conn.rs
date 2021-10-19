use std::convert::TryInto;
use std::{
    collections::HashSet,
    io,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use tokio::sync::watch;

use util::{Buffer, Conn, Error};

use super::socket_addr_ext::{SocketAddrExt, MAX_ADDR_SIZE};
use super::{UDPMuxDefault, RECEIVE_MTU};

#[inline(always)]
/// Create a buffer of appropriate size to fit both a packet with max RECEIVE_MTU and the
/// additional metadata used for muxing.
fn make_buffer() -> Vec<u8> {
    vec![0u8; RECEIVE_MTU + MAX_ADDR_SIZE]
}

#[derive(Debug)]
pub(crate) struct UDPMuxConnParams {
    pub(super) local_addr: Option<SocketAddr>,

    pub(super) key: String,

    // NOTE: This Arc exists in both directions which is liable to cause a retain cycle. This is
    // accounted for in [`UDPMuxDefault::close`], which makes sure to drop all Arcs referencing any
    // `UDPMuxConn`.
    pub(super) udp_mux: Arc<UDPMuxDefault>,
}

#[derive(Debug)]
struct UDPMuxConnInner {
    pub(super) params: UDPMuxConnParams,

    /// Close Sender. We'll send a value on this channel when we close
    closed_watch_tx: Mutex<Option<watch::Sender<bool>>>,

    /// Remote addresses we've seen on this connection.
    addresses: Mutex<HashSet<SocketAddr>>,

    buffer: Buffer,
}

impl UDPMuxConnInner {
    // Sending/Recieving
    async fn recv_from(&self, buf: &mut [u8]) -> ConnResult<(usize, SocketAddr)> {
        // NOTE: Pion/ice uses Sync.Pool to optimise this.
        let mut buffer = make_buffer();
        let mut offset = 0;

        let len = self.buffer.read(&mut buffer, None).await?;
        // We always have at least.
        //
        // * 2 bytes for data len
        // * 2 bytes for addr len
        // * 7 bytes for an Ipv4 addr
        if len < 11 {
            return Err(Error::ErrBufferShort);
        }

        let data_len: usize = *(&buffer[..2]
            .try_into()
            .map(|bytes| u16::from_le_bytes(bytes))
            .map(From::from)
            .unwrap());
        offset += 2;

        let total = usize::from(2 + data_len + 2 + 7);
        if usize::from(data_len) > buf.len() || total > len {
            return Err(Error::ErrBufferShort);
        }

        buf.copy_from_slice(&buffer[offset..offset + data_len]);
        offset += data_len;

        let addr = SocketAddr::decode(&buffer[offset..])?;

        Ok((data_len, addr))
    }

    async fn send_to(&self, buf: &[u8], target: &SocketAddr) -> ConnResult<usize> {
        self.params.udp_mux.send_to(&buf, target).await
    }

    fn is_closed(&self) -> bool {
        self.closed_watch_tx
            .lock()
            .expect("Failed to acquire lock")
            .is_none()
    }

    fn close(self: &Arc<Self>) {
        // TODO: Handle lock error/switch to tokio's Mutex
        let mut closed_tx = self
            .closed_watch_tx
            .lock()
            .expect("Failed to acquire lock.");

        if let Some(tx) = closed_tx.take() {
            let _ = tx.send(true);
            drop(closed_tx);

            let cloned_self = Arc::clone(self);

            {
                let mut addresses = self
                    .addresses
                    .lock()
                    .expect("Failed to obtain addresses lock");
                *addresses = Default::default();
            }

            // NOTE: Alternatively we could wait on the buffer closing here so that
            // our caller can wait for things to fully settle down
            tokio::spawn(async move {
                cloned_self.buffer.close().await;
            });
        }
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        self.params.local_addr
    }

    // Address related methods
    pub(super) fn get_addresses(&self) -> Vec<SocketAddr> {
        let addresses = self.addresses.lock().expect("Failed to obtain lock");

        addresses.iter().cloned().collect()
    }

    pub(super) fn add_address(self: &Arc<Self>, addr: SocketAddr) {
        {
            let mut addresses = self.addresses.lock().expect("Failed to obtain lock");
            addresses.insert(addr.clone());
        }
    }

    pub(super) fn remove_address(&self, addr: &SocketAddr) {
        {
            let mut addresses = self.addresses.lock().expect("Failed to obtain lock");
            addresses.remove(addr);
        }
    }

    pub(super) fn contains_address(&self, addr: &SocketAddr) -> bool {
        let addresses = self.addresses.lock().expect("Failed to obtain lock");

        addresses.contains(addr)
    }
}

#[derive(Debug)]
pub(crate) struct UDPMuxConn {
    /// Close Receiver. A copy of this can be obtained via [`close_tx`].
    closed_watch_rx: watch::Receiver<bool>,

    inner: Arc<UDPMuxConnInner>,
}

impl Clone for UDPMuxConn {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            closed_watch_rx: self.closed_watch_rx.clone(),
        }
    }
}

impl UDPMuxConn {
    pub(crate) fn new(params: UDPMuxConnParams) -> Self {
        let (closed_watch_tx, closed_watch_rx) = watch::channel(false);

        Self {
            closed_watch_rx,
            inner: Arc::new(UDPMuxConnInner {
                params,
                closed_watch_tx: Mutex::new(Some(closed_watch_tx)),
                addresses: Default::default(),
                buffer: Buffer::new(0, 0),
            }),
        }
    }

    pub(crate) fn key(&self) -> &str {
        &self.inner.params.key
    }

    pub(crate) async fn write_packet(&self, data: &[u8], addr: SocketAddr) -> ConnResult<()> {
        // NOTE: Pion/ice uses Sync.Pool to optimise this.
        let mut buffer = make_buffer();
        let mut offset = 0;

        if data.len() + MAX_ADDR_SIZE >= RECEIVE_MTU + MAX_ADDR_SIZE {
            return Err(Error::ErrBufferShort);
        }

        // Format of buffer: | data len(2) | data bytes(dn) | addr len(2) | addr bytes(an) |
        // Where the number in parenthesis indicate the number of bytes used
        // `dn` and `an` are the length in bytes of data and addr respectively.

        // SAFETY: `data.len()` is at most RECEIVE_MTU(8192) - MAX_ADDR_SIZE(512)
        buffer[0..2].copy_from_slice(&(data.len() as u16).to_le_bytes()[..]);
        offset += 2;

        buffer[offset..].copy_from_slice(data);
        offset += data.len();

        let len = addr.encode(&mut buffer[offset + 2..])?;
        buffer[offset..offset + 2].copy_from_slice(&(len as u16).to_le_bytes()[..]);
        offset += 2 + len;

        self.inner.buffer.write(&buffer[..offset]).await?;

        Ok(())
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Get a copy of the close [`tokio::sync::watch::Receiver`] that fires when this
    /// connection is closed.
    pub(crate) fn close_rx(&self) -> watch::Receiver<bool> {
        self.closed_watch_rx.clone()
    }

    /// Close this connection
    pub(crate) fn close(&self) {
        self.inner.close();
    }

    pub(super) fn get_addresses(&self) -> Vec<SocketAddr> {
        self.inner.get_addresses()
    }

    pub(super) fn add_address(&self, addr: SocketAddr) {
        self.inner.add_address(addr);
        self.inner
            .params
            .udp_mux
            .register_conn_for_address(self, addr);
    }

    pub(super) fn remove_address(&self, addr: &SocketAddr) {
        self.inner.remove_address(addr)
    }

    pub(super) fn contains_address(&self, addr: &SocketAddr) -> bool {
        self.inner.contains_address(addr)
    }
}

type ConnResult<T> = Result<T, util::Error>;

#[async_trait]
impl Conn for UDPMuxConn {
    async fn connect(&self, _addr: SocketAddr) -> ConnResult<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    async fn recv(&self, _buf: &mut [u8]) -> ConnResult<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    async fn recv_from(&self, buf: &mut [u8]) -> ConnResult<(usize, SocketAddr)> {
        self.inner.recv_from(buf).await
    }

    async fn send(&self, _buf: &[u8]) -> ConnResult<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> ConnResult<usize> {
        if self.is_closed() {
            return Err(Error::ErrUseClosedNetworkConn);
        }

        if !self.contains_address(&target) {
            self.add_address(target.clone());
        }

        self.inner.send_to(&buf, &target).await
    }

    async fn local_addr(&self) -> ConnResult<SocketAddr> {
        Err(io::Error::new(io::ErrorKind::AddrNotAvailable, "Addr Not Available").into())
    }

    async fn remote_addr(&self) -> Option<SocketAddr> {
        self.inner.remote_addr()
    }
    async fn close(&self) -> ConnResult<()> {
        self.inner.close();

        Ok(())
    }
}
