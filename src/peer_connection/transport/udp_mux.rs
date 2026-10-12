//! UDP multiplexing: many peer connections sharing one UDP socket.
//!
//! A deployment that pins [`with_udp_addrs`](crate::peer_connection::PeerConnectionBuilder::with_udp_addrs)
//! to a fixed port can serve exactly one connection at a time, because every connection binds
//! that port for itself and the second bind fails with `EADDRINUSE`. Servers such as SRS or
//! mediamtx instead expose a **single** UDP port for all of WebRTC: one socket is bound once,
//! and inbound datagrams are demultiplexed to their connection by the ICE username fragment —
//! every STUN connectivity check carries `USERNAME = "local-ufrag:remote-ufrag"`, whose first
//! half names the *receiving* connection — and, once a flow is established, by the peer's
//! address alone.
//!
//! [`UDPMuxDefault`] is that socket. Create it once at startup, hand it to every
//! [`PeerConnectionBuilder`](crate::peer_connection::PeerConnectionBuilder) via
//! [`with_udp_mux`](crate::peer_connection::PeerConnectionBuilder::with_udp_mux), and open or
//! forward exactly one UDP port:
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use webrtc::peer_connection::transport::udp_mux::UDPMuxDefault;
//! # use webrtc::peer_connection::{PeerConnectionBuilder, PeerConnectionEventHandler};
//! # struct MyHandler;
//! # #[async_trait::async_trait]
//! # impl PeerConnectionEventHandler for MyHandler {}
//! # async fn example() -> webrtc::error::Result<()> {
//! let socket = std::net::UdpSocket::bind("0.0.0.0:8189")?;
//! let mux = UDPMuxDefault::new_default_runtime(socket)?;
//!
//! let pc = PeerConnectionBuilder::<&'static str>::new()
//!     .with_handler(Arc::new(MyHandler))
//!     .with_udp_mux(mux.clone())
//!     .build()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Constraints
//!
//! * **Credentials are pinned per connection.** Routing is by ufrag, so a muxed connection's
//!   ICE credentials must be known before signaling and must not change across ICE restarts.
//!   `build()` generates a random pair and injects it with
//!   [`set_ice_credentials`](rtc::peer_connection::configuration::setting_engine::SettingEngine::set_ice_credentials);
//!   setting credentials explicitly *and* using a mux is rejected, since a duplicated ufrag
//!   would route two connections' packets to the same place.
//! * **Host candidates only.** Server-reflexive and relayed gathering send STUN/TURN traffic
//!   to a small set of server addresses, and two connections behind one mux addressing the
//!   same STUN server would collide in the address table. A muxed connection therefore
//!   gathers host candidates only; configure TURN on the *peer* side (or not at all) instead.
//!   This is the same restriction Pion's `ICEUDPMux` documents, and matches ICE Lite servers,
//!   which never gather anything else.
//! * **Wildcard listens need packet info.** Candidates are advertised per interface
//!   (`192.168.1.2:8189`, …), so an inbound datagram must carry its real destination IP to be
//!   matched to a pair. That comes from `IP_PKTINFO`/`RecvMeta::dst_ip`, available on Linux,
//!   Windows, macOS and the BSDs; where it is missing, bind the mux to a concrete interface
//!   address instead of `0.0.0.0`.
//! * **ICE restarts keep the credentials.** Pinning is what makes the route survive a restart,
//!   and the price is that the peer observes no ufrag/pwd rotation (RFC 8445 restarts normally
//!   rotate them). The restarted generation still re-gathers and re-checks, and the transport
//!   genuinely never changed, so there is nothing for the peer to re-pair with.

use std::collections::HashMap;
use std::fmt;
use std::io::{self, IoSliceMut};
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::task::{Context, Poll};

use futures::Stream;
use log::{trace, warn};
use rtc::stun::attributes::ATTR_USERNAME;
use rtc::stun::message::{Message as StunMessage, is_stun_message};

use super::{gro_recv_buf_len, is_retryable_socket_recv_error};
#[cfg(any(feature = "runtime-tokio", feature = "runtime-smol"))]
use crate::runtime::default_runtime;
use crate::runtime::{AsyncUdpSocket, RecvMeta, Runtime, Transmit};

/// Inbound datagrams queued per connection. Media has no flow control to absorb a stalled
/// consumer — the queue exists to ride out scheduling bursts, so past its capacity the mux
/// drops, as UDP itself would.
const MUXED_CONN_CHANNEL_CAPACITY: usize = 512;

/// One inbound datagram routed to a connection.
#[derive(Debug)]
struct MuxedPacket {
    data: Box<[u8]>,
    peer_addr: SocketAddr,
    /// The datagram's real destination IP, from `IP_PKTINFO` where the platform supplies it.
    /// A wildcard-bound mux socket needs it to tell one interface's candidate from another's.
    dst_ip: Option<IpAddr>,
}

/// A UDP socket shared by many peer connections, demultiplexing inbound datagrams by ICE
/// ufrag and peer address.
///
/// Constructed once per process (or per listen port) and shared by every
/// [`PeerConnectionBuilder::with_udp_mux`](crate::peer_connection::PeerConnectionBuilder::with_udp_mux)
/// call. See the [module documentation](self) for the constraints.
///
/// The trait is the seam for custom muxes (an existing socket pump, a tests-only fake, …);
/// [`UDPMuxDefault`] is the built-in implementation.
pub trait UDPMux: fmt::Debug + Send + Sync + 'static {
    /// The address the shared socket is bound to.
    fn local_addr(&self) -> io::Result<SocketAddr>;

    /// Registers a **new** connection under `ufrag` and returns its view of the shared
    /// socket.
    ///
    /// Fails with [`io::ErrorKind::AlreadyExists`] when the ufrag is taken: two connections
    /// registered under one ufrag would share one route, so the duplicate must be refused
    /// rather than silently merged.
    fn register_conn(&self, ufrag: &str) -> io::Result<Arc<dyn AsyncUdpSocket>>;

    /// Re-attaches to the connection registered under `ufrag`, creating it if absent.
    ///
    /// This is the ICE-restart path: a rebind re-registers under the same pinned credentials,
    /// so unlike [`register_conn`](Self::register_conn) an existing registration is returned,
    /// not refused.
    fn get_conn(&self, ufrag: &str) -> io::Result<Arc<dyn AsyncUdpSocket>>;

    /// Deregisters the connection under `ufrag`, if any. Its queued datagrams are dropped and
    /// its peer-address routes are removed.
    fn remove_conn(&self, ufrag: &str);

    /// Closes the mux: every registered connection is dropped and the reader stops.
    fn close(&self);
}

/// The built-in [`UDPMux`]: one bound socket, one reader task, a ufrag table for STUN and a
/// peer-address table for everything else.
///
/// Dropping the last handle stops the reader; [`UDPMux::close`] does the same explicitly.
pub struct UDPMuxDefault {
    socket: Arc<dyn AsyncUdpSocket>,
    local_addr: SocketAddr,
    /// Registered connections by their ICE username fragment.
    conns: Mutex<HashMap<String, Arc<UdpMuxedConn>>>,
    /// Learnt peer-address → connection routes. Filled when a connection *sends* to a peer
    /// and when an inbound STUN check resolves to a connection by ufrag, so established flows
    /// (DTLS, SRTP, SRTCP — none of which carry a ufrag) route without parsing. Values are
    /// weak: a route to a forgotten connection lapses instead of leaking.
    address_map: Arc<Mutex<HashMap<SocketAddr, Weak<UdpMuxedConn>>>>,
    closed: AtomicBool,
}

impl fmt::Debug for UDPMuxDefault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UDPMuxDefault")
            .field("local_addr", &self.local_addr)
            .field("closed", &self.closed.load(Ordering::Relaxed))
            .finish()
    }
}

impl UDPMuxDefault {
    /// Creates a mux over `socket` on the given runtime and starts its reader task.
    ///
    /// The socket should be bound already; it is wrapped with the runtime's usual GSO/GRO
    /// setup. A wildcard bind (`0.0.0.0:port`) serves every interface of that family — see the
    /// module documentation for what that requires of the platform.
    pub fn new(runtime: Arc<dyn Runtime>, socket: std::net::UdpSocket) -> io::Result<Arc<Self>> {
        let wrapped = runtime.wrap_udp_socket(socket)?;
        Self::from_socket(runtime, wrapped)
    }

    /// Creates a mux over an already-wrapped socket. Equivalent to [`new`](Self::new) for
    /// sockets that did not come from `std::net::UdpSocket::bind` directly.
    pub fn from_socket(
        runtime: Arc<dyn Runtime>,
        socket: Arc<dyn AsyncUdpSocket>,
    ) -> io::Result<Arc<Self>> {
        let local_addr = socket.local_addr()?;
        let mux = Arc::new(Self {
            socket,
            local_addr,
            conns: Mutex::new(HashMap::new()),
            address_map: Arc::new(Mutex::new(HashMap::new())),
            closed: AtomicBool::new(false),
        });
        // The reader holds the mux weakly, so dropping the last handle ends the task —
        // `close()` is an explicit shutdown, not a leak plug.
        runtime.spawn(Box::pin(Self::read_loop(Arc::downgrade(&mux))));
        Ok(mux)
    }

    /// [`new`](Self::new) on the compiled-in default runtime. Convenience for the common case;
    /// applications that select a runtime explicitly should call `new` with it.
    #[cfg(any(feature = "runtime-tokio", feature = "runtime-smol"))]
    pub fn new_default_runtime(socket: std::net::UdpSocket) -> io::Result<Arc<Self>> {
        let runtime =
            default_runtime().ok_or_else(|| io::Error::other("no async runtime found"))?;
        Self::new(runtime, socket)
    }

    /// Shared constructor for [`register_conn`](UDPMux::register_conn) and
    /// [`get_conn`](UDPMux::get_conn): builds and registers the conn. Callers hold the `conns`
    /// lock and have already decided whether an existing ufrag is an error or a re-attach.
    fn insert_conn(
        &self,
        conns: &mut HashMap<String, Arc<UdpMuxedConn>>,
        ufrag: &str,
    ) -> Arc<UdpMuxedConn> {
        let (tx, rx) = async_channel::bounded(MUXED_CONN_CHANNEL_CAPACITY);
        let conn = Arc::new(UdpMuxedConn {
            ufrag: ufrag.to_owned(),
            local_addr: self.local_addr,
            socket: Arc::clone(&self.socket),
            address_map: Arc::clone(&self.address_map),
            rx: Mutex::new(Box::pin(rx)),
            tx,
            me: OnceLock::new(),
        });
        let _ = conn.me.set(Arc::downgrade(&conn));
        conns.insert(ufrag.to_owned(), conn.clone());
        conn
    }

    /// The reader: pull datagrams off the shared socket and route each to its connection.
    ///
    /// GRO may coalesce several datagrams of one flow into a single receive; they are split by
    /// `stride` and routed individually, since each is a separate packet for the connection.
    async fn read_loop(mux: Weak<Self>) {
        let Some(this) = mux.upgrade() else { return };
        let socket = Arc::clone(&this.socket);
        let mut buf = vec![0u8; gro_recv_buf_len(socket.max_gro_segments())];
        drop(this);

        loop {
            let mut meta = [RecvMeta::default(); 1];
            let recv = futures::future::poll_fn(|cx| {
                let mut bufs = [IoSliceMut::new(buf.as_mut_slice())];
                socket.poll_recv(cx, &mut bufs, &mut meta)
            })
            .await;

            let Some(this) = mux.upgrade() else { return };
            match recv {
                Ok(_) => {
                    let meta = meta[0];
                    // Split a (possibly GRO-coalesced) receive into its datagrams; the last
                    // may be shorter than `stride`.
                    let step = meta.stride.max(1);
                    let mut off = 0;
                    while off < meta.len {
                        let end = (off + step).min(meta.len);
                        this.route(&buf[off..end], meta.addr, meta.dst_ip);
                        off = end;
                    }
                }
                Err(err) if is_retryable_socket_recv_error(&err) => continue,
                Err(err) => {
                    warn!("udp_mux: socket read failed, reader stops: {err}");
                    return;
                }
            }
        }
    }

    /// Route one datagram to its connection.
    ///
    /// A STUN message carries its destination's identity (the USERNAME's local half) and wins
    /// over the learnt-address table: a client that reconnects from the same address with
    /// fresh credentials must reach its *new* connection, not the stale route. Everything
    /// else — DTLS, SRTP, SRTCP, and STUN without a resolvable USERNAME (binding responses) —
    /// routes by the address learnt when the connection last sent to or was matched for that
    /// peer.
    fn route(&self, data: &[u8], peer_addr: SocketAddr, dst_ip: Option<IpAddr>) {
        // The v6 dual-stack view of an IPv4 peer is the v4-mapped address; connections and
        // ICE candidates both speak plain IPv4, so routes are keyed and delivered in the
        // mapped-back form.
        let peer_addr = normalize_inbound_addr(peer_addr);

        if is_stun_message(data)
            && let Some(ufrag) = stun_username(data)
            && let Some(conn) = self.conns.lock().unwrap().get(ufrag.as_str()).cloned()
        {
            // (Re)point the address route at the connection the check actually names.
            self.address_map
                .lock()
                .unwrap()
                .insert(peer_addr, Arc::downgrade(&conn));
            conn.deliver(data, peer_addr, dst_ip);
            return;
        }

        if let Some(conn) = self
            .address_map
            .lock()
            .unwrap()
            .get(&peer_addr)
            .and_then(Weak::upgrade)
        {
            conn.deliver(data, peer_addr, dst_ip);
            return;
        }

        trace!(
            "udp_mux: dropping {} bytes from unknown {peer_addr}",
            data.len()
        );
    }
}

impl UDPMux for UDPMuxDefault {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn register_conn(&self, ufrag: &str) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        if self.closed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "udp mux is closed",
            ));
        }

        let mut conns = self.conns.lock().unwrap();
        if conns.contains_key(ufrag) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("ICE ufrag {ufrag:?} is already registered on the UDP mux"),
            ));
        }
        Ok(self.insert_conn(&mut conns, ufrag))
    }

    fn get_conn(&self, ufrag: &str) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        if self.closed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "udp mux is closed",
            ));
        }

        let mut conns = self.conns.lock().unwrap();
        if let Some(conn) = conns.get(ufrag) {
            return Ok(conn.clone());
        }
        Ok(self.insert_conn(&mut conns, ufrag))
    }

    fn remove_conn(&self, ufrag: &str) {
        let removed = self.conns.lock().unwrap().remove(ufrag);
        if let Some(conn) = removed {
            // Drop queued datagrams and wake any poll of the connection, then purge its
            // peer-address routes. Entries are matched by identity, not address value, so a
            // route a *newer* connection has since taken over is left alone.
            conn.close();
            self.address_map
                .lock()
                .unwrap()
                .retain(|_, route| route.upgrade().is_none_or(|c| !Arc::ptr_eq(&c, &conn)));
        }
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        for (_, conn) in self.conns.lock().unwrap().drain() {
            conn.close();
        }
        self.address_map.lock().unwrap().clear();
    }
}

/// The local half of a STUN message's USERNAME — the receiving connection's ufrag.
///
/// `None` for anything that is not a decodable message with a UTF-8 USERNAME: routing must
/// never guess, so undecodable traffic simply goes nowhere.
fn stun_username(data: &[u8]) -> Option<String> {
    let mut message = StunMessage::new();
    message.unmarshal_binary(data).ok()?;
    let username = message.get(ATTR_USERNAME).ok()?;
    let username = std::str::from_utf8(&username).ok()?;
    // "local-ufrag:remote-ufrag" from the sender's perspective; the local half names us.
    // ice-char (RFC 8445) excludes ':', so the first segment is the whole local ufrag.
    username.split(':').next().map(str::to_owned)
}

/// The v6 dual-stack view of an IPv4 peer is the v4-mapped address; connections and ICE
/// candidates both speak plain IPv4, so routes are keyed by the mapped-back form.
fn normalize_inbound_addr(addr: SocketAddr) -> SocketAddr {
    match addr {
        SocketAddr::V6(v6) => match v6.ip().to_ipv4_mapped() {
            Some(v4) => SocketAddr::new(IpAddr::V4(v4), v6.port()),
            None => addr,
        },
        v4 => v4,
    }
}

/// A connection's view of the shared socket: sends go straight out, receives arrive through
/// the mux's routing. Learning a peer's address happens here on send — a connection's first
/// outbound datagram to a peer is what lets later non-STUN traffic from it route back.
struct UdpMuxedConn {
    ufrag: String,
    local_addr: SocketAddr,
    socket: Arc<dyn AsyncUdpSocket>,
    address_map: Arc<Mutex<HashMap<SocketAddr, Weak<UdpMuxedConn>>>>,
    /// `&self` polling needs the receiver behind a lock; async-channel's waker registration
    /// survives the guard's drop, so a `Pending` here still wakes the poller. Boxed and pinned
    /// because `async_channel::Receiver` is `!Unpin`.
    rx: Mutex<Pin<Box<async_channel::Receiver<MuxedPacket>>>>,
    /// Kept so `close` can hang up the queue (dropping it closes the channel).
    tx: async_channel::Sender<MuxedPacket>,
    /// Self-reference for registering peer-address routes on send, installed by
    /// [`UDPMuxDefault::get_conn`] once the `Arc` exists.
    me: OnceLock<Weak<UdpMuxedConn>>,
}

impl fmt::Debug for UdpMuxedConn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdpMuxedConn")
            .field("ufrag", &self.ufrag)
            .field("local_addr", &self.local_addr)
            .finish()
    }
}

impl UdpMuxedConn {
    fn deliver(&self, data: &[u8], peer_addr: SocketAddr, dst_ip: Option<IpAddr>) {
        let packet = MuxedPacket {
            data: Box::from(data),
            peer_addr,
            dst_ip,
        };
        match self.tx.try_send(packet) {
            Ok(()) => {}
            Err(async_channel::TrySendError::Full(_)) => {
                trace!(
                    "udp_mux: connection {} is not keeping up, dropping datagram from {peer_addr}",
                    self.ufrag
                );
            }
            Err(async_channel::TrySendError::Closed(_)) => {}
        }
    }

    fn close(&self) {
        self.tx.close();
    }
}

impl AsyncUdpSocket for UdpMuxedConn {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn poll_send(&self, cx: &mut Context<'_>, transmit: &Transmit<'_>) -> Poll<io::Result<usize>> {
        // Learn the route before sending, so a response that beats our return already knows
        // where to go. A newer connection claiming the address wins (see `route`).
        if let Some(me) = self.me.get().cloned() {
            self.address_map
                .lock()
                .unwrap()
                .insert(normalize_inbound_addr(transmit.destination), me);
        }
        self.socket.poll_send(cx, transmit)
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let mut rx = self.rx.lock().unwrap();
        match rx.as_mut().poll_next(cx) {
            Poll::Ready(Some(packet)) => {
                let n = packet.data.len().min(bufs[0].len());
                if n < packet.data.len() {
                    warn!(
                        "udp_mux: truncating {}-byte datagram from {} to {} bytes",
                        packet.data.len(),
                        packet.peer_addr,
                        n
                    );
                }
                bufs[0][..n].copy_from_slice(&packet.data[..n]);
                let m = &mut meta[0];
                m.addr = packet.peer_addr;
                m.len = n;
                // One datagram per read, so the buffer's stride is its length; GRO splitting
                // happened in the mux's reader already.
                m.stride = n.max(1);
                m.ecn = None;
                m.dst_ip = packet.dst_ip;
                Poll::Ready(Ok(1))
            }
            // The mux closed the connection: report the socket gone, like an unplugged NIC.
            Poll::Ready(None) => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "udp mux connection is closed",
            ))),
            Poll::Pending => Poll::Pending,
        }
    }

    /// GSO is inherited from the shared socket: batched same-flow datagrams are passed to it
    /// verbatim, so its segment limit applies unchanged.
    fn max_gso_segments(&self) -> usize {
        self.socket.max_gso_segments()
    }

    /// The shared socket's GRO figure, not this connection's (which de-segments in the mux's
    /// reader): reporting it sizes the driver's receive buffer to hold the largest single
    /// datagram the reader can dequeue.
    fn max_gro_segments(&self) -> usize {
        self.socket.max_gro_segments()
    }
}

#[cfg(test)]
mod tests {
    //! Routing and lifecycle tests, driven synchronously: `route` is called by hand rather
    //! than through the reader task, so no runtime is needed. The shared socket is a fake;
    //! what is under test is the demux table, not I/O.
    use super::*;
    use crate::runtime::poll_once;
    use rtc::stun::message::{BINDING_REQUEST, TransactionId};
    use rtc::stun::textattrs::Username;

    #[derive(Debug, Default)]
    struct FakeSocket {
        sent: Mutex<Vec<(Vec<u8>, SocketAddr)>>,
    }

    impl AsyncUdpSocket for FakeSocket {
        fn local_addr(&self) -> io::Result<SocketAddr> {
            Ok(SocketAddr::from(([127, 0, 0, 1], 8189)))
        }

        fn poll_send(
            &self,
            _cx: &mut Context<'_>,
            transmit: &Transmit<'_>,
        ) -> Poll<io::Result<usize>> {
            self.sent
                .lock()
                .unwrap()
                .push((transmit.contents.to_vec(), transmit.destination));
            Poll::Ready(Ok(transmit.contents.len()))
        }

        fn poll_recv(
            &self,
            _cx: &mut Context<'_>,
            _bufs: &mut [IoSliceMut<'_>],
            _meta: &mut [RecvMeta],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }
    }

    fn mux() -> Arc<UDPMuxDefault> {
        let socket: Arc<dyn AsyncUdpSocket> = Arc::new(FakeSocket::default());
        Arc::new(UDPMuxDefault {
            local_addr: socket.local_addr().unwrap(),
            socket,
            conns: Mutex::new(HashMap::new()),
            address_map: Arc::new(Mutex::new(HashMap::new())),
            closed: AtomicBool::new(false),
        })
    }

    fn binding_request(username: &str) -> Vec<u8> {
        let mut message = StunMessage::new();
        message
            .build(&[
                Box::new(BINDING_REQUEST),
                Box::<TransactionId>::default(),
                Box::new(Username::new(ATTR_USERNAME, username.to_owned())),
            ])
            .unwrap();
        message.raw
    }

    /// What a routed datagram came out as: `(contents, peer, dst_ip)`; `None` when the
    /// connection's queue is empty.
    fn poll_packet(
        conn: &Arc<dyn AsyncUdpSocket>,
    ) -> Option<(Vec<u8>, SocketAddr, Option<IpAddr>)> {
        let mut buf = [0u8; 2048];
        let mut meta = [RecvMeta::default(); 1];
        poll_once(|cx| {
            let mut bufs = [IoSliceMut::new(&mut buf)];
            conn.poll_recv(cx, &mut bufs, &mut meta)
        })
        .and_then(|result| result.ok())
        .map(|_| (buf[..meta[0].len].to_vec(), meta[0].addr, meta[0].dst_ip))
    }

    fn send_to(conn: &Arc<dyn AsyncUdpSocket>, data: &[u8], target: SocketAddr) {
        let transmit = Transmit {
            destination: target,
            ecn: None,
            contents: data,
            segment_size: None,
            src_ip: None,
        };
        let sent = poll_once(|cx| conn.poll_send(cx, &transmit));
        assert!(matches!(sent, Some(Ok(n)) if n == data.len()));
    }

    fn peer() -> SocketAddr {
        SocketAddr::from(([192, 0, 2, 1], 40000))
    }

    #[test]
    fn routes_a_stun_check_to_the_conn_named_by_the_username() {
        let mux = mux();
        let conn_a = mux.register_conn("aaa").unwrap();
        let conn_b = mux.register_conn("bbb").unwrap();

        // USERNAME is "local-ufrag:remote-ufrag" from the sender's perspective: the first
        // half names the *receiving* connection.
        let packet = binding_request("aaa:remote");
        mux.route(&packet, peer(), Some(IpAddr::from([127, 0, 0, 1])));

        assert_eq!(poll_packet(&conn_b), None, "conn b must see nothing");
        let (data, from, dst_ip) = poll_packet(&conn_a).expect("conn a gets the check");
        assert_eq!(data, packet);
        assert_eq!(from, peer());
        assert_eq!(dst_ip, Some(IpAddr::from([127, 0, 0, 1])));

        // Routing by ufrag also learns the peer address: the next, ufrag-less datagram from
        // the same peer goes straight to the connection without parsing.
        mux.route(b"\x80\x77dtls-not-stun", peer(), None);
        let (data, _, _) = poll_packet(&conn_a).expect("conn a gets the follow-up");
        assert_eq!(data, b"\x80\x77dtls-not-stun");
        assert_eq!(poll_packet(&conn_b), None);
    }

    #[test]
    fn routes_by_the_address_learnt_on_send() {
        let mux = mux();
        let conn_a = mux.register_conn("aaa").unwrap();
        let conn_b = mux.register_conn("bbb").unwrap();

        // The connection's first outbound datagram claims the peer address; DTLS/SRTP from
        // that peer — never carrying a ufrag — routes back to it.
        send_to(&conn_b, b"binding response", peer());
        mux.route(b"\x80\x77dtls-not-stun", peer(), None);

        let (data, _, _) = poll_packet(&conn_b).expect("conn b gets the reply");
        assert_eq!(data, b"\x80\x77dtls-not-stun");
        assert_eq!(poll_packet(&conn_a), None);
    }

    #[test]
    fn drops_packets_from_unknown_peers() {
        let mux = mux();
        let conn_a = mux.register_conn("aaa").unwrap();

        mux.route(b"\x80\x77dtls-not-stun", peer(), None);
        // A STUN check naming a ufrag nobody registered.
        mux.route(&binding_request("zzz:remote"), peer(), None);
        // Garbage that is not STUN at all.
        mux.route(&binding_request("aaa:remote")[..10], peer(), None);

        assert_eq!(poll_packet(&conn_a), None, "nothing may be routed");
    }

    #[test]
    fn a_new_claimant_of_a_peer_address_wins_the_route() {
        // A client that reconnects from the same address with fresh credentials must not be
        // stranded on its previous connection's route: the ufrag match re-points it.
        let mux = mux();
        let conn_a = mux.register_conn("aaa").unwrap();
        let conn_b = mux.register_conn("bbb").unwrap();

        send_to(&conn_a, b"outbound", peer());
        mux.route(&binding_request("bbb:remote"), peer(), None);

        // The route has moved: the check itself reached the new connection ...
        let (check, _, _) = poll_packet(&conn_b).expect("conn b gets its own check");
        assert_eq!(check, binding_request("bbb:remote"));
        // ... and media from the peer now goes to it as well.
        mux.route(b"\x80\x77dtls-not-stun", peer(), None);
        let (data, _, _) = poll_packet(&conn_b).expect("conn b takes over the peer");
        assert_eq!(data, b"\x80\x77dtls-not-stun");
        assert_eq!(poll_packet(&conn_a), None, "the stale route is gone");
    }

    #[test]
    fn register_conn_refuses_a_duplicate_ufrag() {
        let mux = mux();
        mux.register_conn("aaa").unwrap();
        let dup = mux.register_conn("aaa");
        assert_eq!(dup.unwrap_err().kind(), io::ErrorKind::AlreadyExists);

        // Re-attaching (the ICE-restart path) is not a duplicate.
        assert!(mux.get_conn("aaa").is_ok());
    }

    #[test]
    fn remove_conn_frees_the_ufrag_and_its_routes() {
        let mux = mux();
        let conn_a = mux.register_conn("aaa").unwrap();
        send_to(&conn_a, b"outbound", peer());

        mux.remove_conn("aaa");
        assert!(mux.register_conn("aaa").is_ok(), "the ufrag is free again");

        // The freed connection's route is gone with it: nothing routes anywhere now.
        mux.route(b"\x80\x77dtls-not-stun", peer(), None);
        assert_eq!(poll_packet(&conn_a), None);
    }

    #[test]
    fn remove_conn_keeps_routes_a_new_conn_has_taken_over() {
        let mux = mux();
        let conn_a = mux.register_conn("aaa").unwrap();
        let conn_b = mux.register_conn("bbb").unwrap();

        send_to(&conn_a, b"outbound", peer());
        // conn b takes the peer over (its own outbound learn), then conn a is removed:
        // conn b's route must survive the purge.
        send_to(&conn_b, b"outbound", peer());
        mux.remove_conn("aaa");

        mux.route(b"\x80\x77dtls-not-stun", peer(), None);
        let (data, _, _) = poll_packet(&conn_b).expect("conn b keeps its route");
        assert_eq!(data, b"\x80\x77dtls-not-stun");
    }

    #[test]
    fn close_hangs_up_every_conn_and_rejects_registrations() {
        let mux = mux();
        let conn_a = mux.register_conn("aaa").unwrap();
        let conn_b = mux.register_conn("bbb").unwrap();

        mux.close();

        for conn in [&conn_a, &conn_b] {
            let mut buf = [0u8; 64];
            let mut meta = [RecvMeta::default(); 1];
            let result = poll_once(|cx| {
                let mut bufs = [IoSliceMut::new(&mut buf)];
                conn.poll_recv(cx, &mut bufs, &mut meta)
            });
            assert_eq!(
                result.and_then(|r| r.err()).map(|e| e.kind()),
                Some(io::ErrorKind::BrokenPipe),
                "a closed mux's conns report the socket gone"
            );
        }
        assert!(mux.register_conn("ccc").is_err());
    }

    #[test]
    fn v4_mapped_v6_peer_addresses_normalize_to_v4_routes() {
        // A dual-stack mux socket reports IPv4 peers as v4-mapped IPv6; candidates and
        // routes both speak plain IPv4, so the mapping must be undone on the way in.
        let mux = mux();
        let conn_a = mux.register_conn("aaa").unwrap();
        let v6_mapped: SocketAddr = "[::ffff:192.0.2.1]:40000".parse().unwrap();

        mux.route(&binding_request("aaa:remote"), v6_mapped, None);
        let (_, from, _) = poll_packet(&conn_a).expect("conn a gets the check");
        assert_eq!(from, peer(), "the conn sees the plain IPv4 address");

        // And the learnt route matches the plain form too.
        mux.route(b"\x80\x77dtls-not-stun", peer(), None);
        assert!(poll_packet(&conn_a).is_some());
    }

    #[test]
    fn stun_username_reads_the_local_half() {
        assert_eq!(
            stun_username(&binding_request("local:remote")),
            Some("local".to_owned())
        );
        // No USERNAME, not STUN, truncated: nothing to route by.
        let mut no_username = StunMessage::new();
        no_username
            .build(&[Box::new(BINDING_REQUEST), Box::<TransactionId>::default()])
            .unwrap();
        assert_eq!(stun_username(&no_username.raw), None);
        assert_eq!(stun_username(b"not stun"), None);
        assert_eq!(stun_username(&binding_request("a:b")[..10]), None);
    }
}
