use std::collections::HashSet;
use std::convert::TryInto;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use tokio::sync::watch;
use util::sync::Mutex;
use util::{Buffer, Conn, Error};

use super::socket_addr_ext::{SocketAddrExt, MAX_ADDR_SIZE};
use super::{normalize_socket_addr, RECEIVE_MTU};

/// A trait for a [`UDPMuxConn`] to communicate with an UDP mux.
#[async_trait]
pub trait UDPMuxWriter {
    /// Registers an address for the given connection.
    async fn register_conn_for_address(&self, conn: &UDPMuxConn, addr: SocketAddr);
    /// Sends the content of the buffer to the given target.
    ///
    /// Returns the number of bytes sent or an error, if any.
    async fn send_to(&self, buf: &[u8], target: &SocketAddr) -> Result<usize, Error>;
}

/// Parameters for a [`UDPMuxConn`].
pub struct UDPMuxConnParams {
    /// Local socket address.
    pub local_addr: SocketAddr,
    /// Static key identifying the connection.
    pub key: String,
    /// A `std::sync::Weak` reference to the UDP mux.
    ///
    /// NOTE: a non-owning reference should be used to prevent possible cycles.
    pub udp_mux: Weak<dyn UDPMuxWriter + Send + Sync>,
}

type ConnResult<T> = Result<T, util::Error>;

/// A UDP mux connection.
#[derive(Clone)]
pub struct UDPMuxConn {
    /// Close Receiver. A copy of this can be obtained via [`close_tx`].
    closed_watch_rx: watch::Receiver<bool>,

    inner: Arc<UDPMuxConnInner>,
}

impl UDPMuxConn {
    /// Creates a new [`UDPMuxConn`].
    pub fn new(params: UDPMuxConnParams) -> Self {
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

    /// Returns a key identifying this connection.
    pub fn key(&self) -> &str {
        &self.inner.params.key
    }

    /// Writes data to the given address. Returns an error if the buffer is too short or there's an
    /// encoding error.
    pub async fn write_packet(&self, data: &[u8], addr: SocketAddr) -> ConnResult<()> {
        // NOTE: Pion/ice uses Sync.Pool to optimise this.
        let mut buffer = make_buffer();
        let mut offset = 0;

        if (data.len() + MAX_ADDR_SIZE) > (RECEIVE_MTU + MAX_ADDR_SIZE) {
            return Err(Error::ErrBufferShort);
        }

        // Format of buffer: | data len(2) | data bytes(dn) | addr len(2) | addr bytes(an) |
        // Where the number in parenthesis indicate the number of bytes used
        // `dn` and `an` are the length in bytes of data and addr respectively.

        // SAFETY: `data.len()` is at most RECEIVE_MTU(8192) - MAX_ADDR_SIZE(27)
        buffer[0..2].copy_from_slice(&(data.len() as u16).to_le_bytes()[..]);
        offset += 2;

        buffer[offset..offset + data.len()].copy_from_slice(data);
        offset += data.len();

        let len = addr.encode(&mut buffer[offset + 2..])?;
        buffer[offset..offset + 2].copy_from_slice(&(len as u16).to_le_bytes()[..]);
        offset += 2 + len;

        self.inner.buffer.write(&buffer[..offset]).await?;

        Ok(())
    }

    /// Returns true if this connection is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Gets a copy of the close [`tokio::sync::watch::Receiver`] that fires when this
    /// connection is closed.
    pub fn close_rx(&self) -> watch::Receiver<bool> {
        self.closed_watch_rx.clone()
    }

    /// Closes this connection.
    pub fn close(&self) {
        self.inner.close();
    }

    /// Gets the list of the addresses associated with this connection.
    pub fn get_addresses(&self) -> Vec<SocketAddr> {
        self.inner.get_addresses()
    }

    /// Registers a new address for this connection.
    pub async fn add_address(&self, addr: SocketAddr) {
        self.inner.add_address(addr);
        if let Some(mux) = self.inner.params.udp_mux.upgrade() {
            mux.register_conn_for_address(self, addr).await;
        }
    }

    /// Deregisters an address.
    pub fn remove_address(&self, addr: &SocketAddr) {
        self.inner.remove_address(addr)
    }

    /// Returns true if the given address is associated with this connection.
    pub fn contains_address(&self, addr: &SocketAddr) -> bool {
        self.inner.contains_address(addr)
    }
}

struct UDPMuxConnInner {
    params: UDPMuxConnParams,

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

        let data_len: usize = buffer[..2]
            .try_into()
            .map(u16::from_le_bytes)
            .map(From::from)
            .unwrap();
        offset += 2;

        let total = 2 + data_len + 2 + 7;
        if data_len > buf.len() || total > len {
            return Err(Error::ErrBufferShort);
        }

        buf[..data_len].copy_from_slice(&buffer[offset..offset + data_len]);
        offset += data_len;

        let address_len: usize = buffer[offset..offset + 2]
            .try_into()
            .map(u16::from_le_bytes)
            .map(From::from)
            .unwrap();
        offset += 2;

        let addr = SocketAddr::decode(&buffer[offset..offset + address_len])?;

        Ok((data_len, addr))
    }

    async fn send_to(&self, buf: &[u8], target: &SocketAddr) -> ConnResult<usize> {
        if let Some(mux) = self.params.udp_mux.upgrade() {
            mux.send_to(buf, target).await
        } else {
            Err(Error::Other(format!(
                "wanted to send {} bytes to {}, but UDP mux is gone",
                buf.len(),
                target
            )))
        }
    }

    fn is_closed(&self) -> bool {
        self.closed_watch_tx.lock().is_none()
    }

    fn close(self: &Arc<Self>) {
        let mut closed_tx = self.closed_watch_tx.lock();

        if let Some(tx) = closed_tx.take() {
            let _ = tx.send(true);
            drop(closed_tx);

            let cloned_self = Arc::clone(self);

            {
                let mut addresses = self.addresses.lock();
                *addresses = Default::default();
            }

            // NOTE: Alternatively we could wait on the buffer closing here so that
            // our caller can wait for things to fully settle down
            tokio::spawn(async move {
                cloned_self.buffer.close().await;
            });
        }
    }

    fn local_addr(&self) -> SocketAddr {
        self.params.local_addr
    }

    // Address related methods
    pub(super) fn get_addresses(&self) -> Vec<SocketAddr> {
        let addresses = self.addresses.lock();

        addresses.iter().copied().collect()
    }

    pub(super) fn add_address(self: &Arc<Self>, addr: SocketAddr) {
        {
            let mut addresses = self.addresses.lock();
            addresses.insert(addr);
        }
    }

    pub(super) fn remove_address(&self, addr: &SocketAddr) {
        {
            let mut addresses = self.addresses.lock();
            addresses.remove(addr);
        }
    }

    pub(super) fn contains_address(&self, addr: &SocketAddr) -> bool {
        let addresses = self.addresses.lock();

        addresses.contains(addr)
    }
}

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
        let normalized_target = normalize_socket_addr(&target, &self.inner.params.local_addr);

        if !self.contains_address(&normalized_target) {
            self.add_address(normalized_target).await;
        }

        self.inner.send_to(buf, &normalized_target).await
    }

    fn local_addr(&self) -> ConnResult<SocketAddr> {
        Ok(self.inner.local_addr())
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        None
    }
    async fn close(&self) -> ConnResult<()> {
        self.inner.close();

        Ok(())
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

#[inline(always)]
/// Create a buffer of appropriate size to fit both a packet with max RECEIVE_MTU and the
/// additional metadata used for muxing.
fn make_buffer() -> Vec<u8> {
    // The 4 extra bytes are used to encode the length of the data and address respectively.
    // See [`write_packet`] for details.
    vec![0u8; RECEIVE_MTU + MAX_ADDR_SIZE + 2 + 2]
}
