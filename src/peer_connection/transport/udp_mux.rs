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
//! * **Routing is authenticated.** A STUN check is routed by its ufrag — and is allowed to
//!   learn or move a peer-address route — only when it is a `BINDING_REQUEST` carrying a
//!   `MESSAGE-INTEGRITY` that verifies against the connection's ICE password. Anything else
//!   (forged checks, binding responses/indications, unauthenticated garbage) routes, if at
//!   all, by peer address alone and never mutates the route table.
//! * **Credentials rotate on restart.** The driver learns a connection's ICE credentials from
//!   its *local description* at each gathering phase — the agent's generated pair is only
//!   visible there after the first `set_local_description`, which is also the earliest moment
//!   an inbound check can name the connection (the peer needs our SDP first). On an ICE
//!   restart the freshly rotated ufrag is registered alongside the previous one, which is
//!   kept for a grace period. The peer observes normal RFC 8445 ufrag/pwd rotation.
//!   Application-pinned credentials
//!   ([`SettingEngineBuilder::with_ice_credentials`](rtc::peer_connection::configuration::setting_engine::SettingEngineBuilder::with_ice_credentials))
//!   are registered eagerly at `build` instead — they are known before any SDP exists — so a
//!   duplicate pinned ufrag fails `build` the way a failed bind does.
//! * **Host candidates only.** Server-reflexive and relayed gathering send STUN/TURN traffic
//!   to a small set of server addresses, and two connections behind one mux addressing the
//!   same STUN server would collide in the address table. A muxed connection therefore
//!   gathers host candidates only; configure TURN on the *peer* side (or not at all) instead.
//!   This is the same restriction Pion's `ICEUDPMux` documents, and matches ICE Lite servers,
//!   which never gather anything else. An ICE transport policy of `Relay` is rejected by
//!   `build`, since it would gather nothing at all.
//! * **Wildcard listens need packet info.** Candidates are advertised per interface
//!   (`192.168.1.2:8189`, …), so an inbound datagram must carry its real destination IP to be
//!   matched to a pair. That comes from `IP_PKTINFO`/`RecvMeta::dst_ip`, available on Linux,
//!   Windows, macOS and the BSDs; where it is missing, bind the mux to a concrete interface
//!   address instead of `0.0.0.0`.
//! * **One peer address, one connection.** The peer-address route table is keyed by the peer's
//!   `ip:port` alone, so a single remote address cannot serve two connections: STUN checks are
//!   still separated by ufrag, but DTLS/SRTP/SRTCP (which carry no ufrag) would all route to
//!   whichever connection owns the address. This is the server deployment shape — a mux serves
//!   clients whose connections each use their own socket; do not put *both* ends of a connection
//!   on muxes at the same address (two muxes talking to each other, or a browser multiplexing
//!   several peer connections over one UDP socket).
//! * **smol send back-pressure.** The mux shares one socket across every driver, so many tasks
//!   poll its `poll_send`. Under tokio that is fine (multiple waiters); under smol the async-io
//!   socket keeps a single waker per direction and wakes the displaced waiter, so a full send
//!   buffer makes the drivers re-queue against each other. It is livelock-free but can spin —
//!   prefer tokio for a high-connection mux, or keep per-connection socket buffers large enough
//!   that `send` never waits.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::io::{self, IoSliceMut};
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures::{FutureExt, Stream};
use log::{trace, warn};
use rtc::stun::attributes::ATTR_USERNAME;
use rtc::stun::integrity::MessageIntegrity;
use rtc::stun::message::{BINDING_REQUEST, Message as StunMessage, is_stun_message};

use super::{gro_recv_buf_len, is_retryable_socket_recv_error};
use crate::peer_connection::crypto;
#[cfg(any(feature = "runtime-tokio", feature = "runtime-smol"))]
use crate::runtime::default_runtime;
use crate::runtime::{AsyncUdpSocket, Notify, RecvMeta, Runtime, Transmit};

/// Inbound datagrams queued per connection. Media has no flow control to absorb a stalled
/// consumer — the queue exists to ride out scheduling bursts, so past its capacity the mux
/// drops, as UDP itself would.
const MUXED_CONN_CHANNEL_CAPACITY: usize = 512;

/// Pooled datagram buffers are this big: one WebRTC datagram (MTU 1500) plus headroom. A
/// bigger datagram gets its own allocation, which is never pooled back.
const POOL_BUF_LEN: usize = 2048;

/// Upper bound on the peer addresses one connection learns into the route table. The set is
/// keyed per connection and is the only thing an inbound check or an outbound send can grow;
/// bounding it bounds the global address map by `connections × MAX_LEARNED_ADDRS_PER_CONN`,
/// which is what keeps a peer that floods checks from many source addresses (an IPv6 /64 makes
/// this trivial) from growing the mux's memory without bound. A mobile peer rarely exceeds a
/// handful of NAT rebinds over one connection's life, so the eviction is invisible in practice.
const MAX_LEARNED_ADDRS_PER_CONN: usize = 64;

/// Upper bound on the STUN checks (and therefore HMAC computations) one connection's traffic
/// may drive per second in the reader. A peer that knows its own ufrag can flood checks and make
/// the reader HMAC every one; this caps the CPU that path can consume. ICE connectivity checks
/// run well under this (a burst of pairs still stays below it), so legitimate connections are
/// unaffected.
const MAX_CHECKS_PER_SECOND_PER_CONN: u32 = 512;

/// A fixed-window rate limiter for per-connection STUN check authentication.
struct CheckRateLimiter {
    window_start: Instant,
    checks: u32,
}

impl CheckRateLimiter {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            checks: 0,
        }
    }

    fn allow(&mut self, now: Instant) -> bool {
        if now.duration_since(self.window_start) >= Duration::from_secs(1) {
            self.window_start = now;
            self.checks = 0;
        }
        self.checks += 1;
        self.checks <= MAX_CHECKS_PER_SECOND_PER_CONN
    }
}

/// A freelist of datagram buffers shared between the mux's reader (filling them) and the
/// conns (draining and returning them), so steady-state packet delivery does not allocate.
struct BufPool {
    free: Mutex<Vec<Box<[u8]>>>,
}

impl BufPool {
    /// Take a buffer without ever blocking: reuse a pooled one when one is immediately
    /// available, otherwise allocate fresh. On a busy mux the reader and every driver contend on
    /// this freelist per datagram, so waiting here would serialize the hot path — a rare extra
    /// allocation is far cheaper than parking.
    fn get(&self) -> Box<[u8]> {
        if let Ok(mut free) = self.free.try_lock()
            && let Some(buf) = free.pop()
        {
            return buf;
        }
        vec![0u8; POOL_BUF_LEN].into_boxed_slice()
    }

    /// Return a buffer without ever blocking: push it when the lock is immediately available,
    /// otherwise drop it. Dropping recycles through the allocator rather than stalling a driver
    /// task on the shared freelist.
    fn put(&self, buf: Box<[u8]>) {
        if buf.len() == POOL_BUF_LEN
            && let Ok(mut free) = self.free.try_lock()
        {
            free.push(buf);
        }
    }
}

/// One inbound datagram routed to a connection.
#[derive(Debug)]
struct MuxedPacket {
    /// The datagram's bytes; only `data[..len]` is payload. Pooled — see [`BufPool`].
    data: Box<[u8]>,
    len: usize,
    peer_addr: SocketAddr,
    /// The datagram's real destination IP, from `IP_PKTINFO` where the platform supplies it.
    /// A wildcard-bound mux socket needs it to tell one interface's candidate from another's.
    dst_ip: Option<IpAddr>,
    ecn: Option<crate::runtime::EcnCodepoint>,
}

/// A connection's registration identity in a [`UDPMux`].
///
/// Opaque to callers; handed back by [`UDPMux::register_conn`] and passed to
/// [`UDPMux::add_ufrag`]/[`UDPMux::remove_conn`]. Keying removal by the handle — rather than by
/// ufrag — is what makes a driver's `Drop` remove exactly *its* registration even after the
/// ufrag has been reused by a newer connection on the same mux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UDPMuxConnHandle(u64);

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

    /// Registers a **new** connection and returns its identity and its view of the shared
    /// socket.
    ///
    /// The connection is created without a routing entry: the driver adds its ICE ufrag(s)
    /// afterwards via [`add_ufrag`](UDPMux::add_ufrag), once the credentials are known — the
    /// agent-generated pair is only visible in the local description after the first
    /// `set_local_description`, while no inbound check can name the connection before the peer
    /// has that SDP. Until a ufrag is added, only outbound sends and address-routed inbound
    /// traffic work.
    fn register_conn(&self) -> io::Result<(UDPMuxConnHandle, Arc<dyn AsyncUdpSocket>)>;

    /// Adds `ufrag` to a registered connection — its first routing entry, or a second one
    /// alongside the previous during an ICE restart, so it routes checks under both ufrags.
    ///
    /// `pwd` is the connection's ICE password; the reader uses it to verify the
    /// `MESSAGE-INTEGRITY` of STUN checks before trusting their ufrag. This is how an ICE
    /// restart moves a connection to its freshly rotated credentials without tearing down its
    /// socket view or queue. Fails with [`io::ErrorKind::AlreadyExists`] when the ufrag is
    /// taken — two connections registered under one ufrag would share one route, so the
    /// duplicate must be refused rather than silently merged — and with
    /// [`io::ErrorKind::InvalidInput`] for an unknown handle or a malformed ufrag.
    fn add_ufrag(&self, handle: UDPMuxConnHandle, ufrag: &str, pwd: &str) -> io::Result<()>;

    /// Removes one ufrag from a connection (an ICE-restart grace period ending). The connection
    /// and its peer-address routes are left intact; only checks naming that ufrag stop routing.
    fn remove_ufrag(&self, ufrag: &str);

    /// Deregisters the connection identified by `handle`. Its queued datagrams are dropped and
    /// its peer-address routes are removed.
    fn remove_conn(&self, handle: UDPMuxConnHandle);

    /// Closes the mux: every registered connection is dropped and the reader stops.
    fn close(&self);

    /// How long an ICE-restarted ufrag stays routable after it is rotated away.
    ///
    /// The driver keeps a connection's previous ufrag registered for this long after adding the
    /// fresh one, so checks the peer sends before applying the new SDP still route. The default
    /// (30 s) is more than enough for a re-offer/answer round-trip.
    fn ufrag_grace(&self) -> Duration {
        Duration::from_secs(30)
    }

    /// Number of inbound datagrams this connection dropped because its queue was full or it was
    /// already closed. Monotonic per connection; lets an operator observe mux-level loss.
    fn dropped_datagrams(&self, handle: UDPMuxConnHandle) -> u64;
}

/// The built-in [`UDPMux`]: one bound socket, one reader task, a ufrag table for STUN and a
/// peer-address table for everything else.
///
/// Dropping the last handle closes the mux; [`UDPMux::close`] does the same explicitly.
pub struct UDPMuxDefault {
    socket: Arc<dyn AsyncUdpSocket>,
    local_addr: SocketAddr,
    /// Registered connections by their ICE username fragment. Several ufrags may name one
    /// connection: on an ICE restart the driver adds the new ufrag while the old one is still
    /// within its grace period.
    conns: Mutex<HashMap<String, Arc<UdpMuxedConn>>>,
    /// The same connections by their [`UDPMuxConnHandle`], for handle-keyed removal.
    conns_by_handle: Mutex<HashMap<UDPMuxConnHandle, Arc<UdpMuxedConn>>>,
    /// Monotonic source of [`UDPMuxConnHandle`] tokens.
    next_handle: AtomicU64,
    /// Learnt peer-address → connection routes. Filled when a connection *sends* to a peer
    /// and when an authenticated STUN check resolves to a connection by ufrag, so established
    /// flows (DTLS, SRTP, SRTCP — none of which carry a ufrag) route without parsing. Values
    /// are weak: a route to a forgotten connection lapses instead of leaking.
    address_map: Arc<Mutex<HashMap<SocketAddr, Weak<UdpMuxedConn>>>>,
    /// Datagram buffer pool shared by the reader and every conn.
    pool: Arc<BufPool>,
    /// Verifies the `MESSAGE-INTEGRITY` of STUN checks before their ufrag is trusted.
    crypto: Arc<dyn crypto::RTCCryptoProvider>,
    /// Wakes the reader out of its parked receive on [`close`](UDPMux::close). The reader
    /// parks in the socket's `poll_recv`, which no flag check can interrupt — so close
    /// notifies, and the loop also re-checks the flag on every packet.
    closed_notify: Arc<Notify>,
    closed: AtomicBool,
}

impl Drop for UDPMuxDefault {
    fn drop(&mut self) {
        // Last-handle drop is an implicit close: hang up the conns (their queues would
        // otherwise park on a receiver nobody feeds), and wake the reader out of its parked
        // poll_recv — it holds the socket, not the mux, so without the wake the port stays
        // bound. The flag matters as much as the notification: a reader that has not created
        // its listener yet must still observe the shutdown.
        self.closed.store(true, Ordering::Release);
        for (_, conn) in self.conns_by_handle.get_mut().unwrap().drain() {
            conn.close();
        }
        self.closed_notify.notify_waiters();
    }
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
    ///
    /// The caller is responsible for the socket's platform options: a wildcard bind needs the
    /// packet-info options that populate `RecvMeta::dst_ip` (`IP_PKTINFO`/`IPV6_RECVPKTINFO`,
    /// or their equivalents), or inbound datagrams cannot be attributed to the interface they
    /// arrived on and will not match a host candidate. [`new`](Self::new) sets these up.
    pub fn from_socket(
        runtime: Arc<dyn Runtime>,
        socket: Arc<dyn AsyncUdpSocket>,
    ) -> io::Result<Arc<Self>> {
        let local_addr = socket.local_addr()?;
        let crypto = crypto::default_provider().map_err(|error| {
            io::Error::other(format!(
                "udp mux needs a crypto provider to verify STUN checks: {error}"
            ))
        })?;
        let mux = Arc::new(Self {
            socket,
            local_addr,
            conns: Mutex::new(HashMap::new()),
            conns_by_handle: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(0),
            address_map: Arc::new(Mutex::new(HashMap::new())),
            pool: Arc::new(BufPool {
                free: Mutex::new(Vec::new()),
            }),
            crypto,
            closed_notify: Arc::new(Notify::new()),
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

    /// The reader: pull datagrams off the shared socket and route each to its connection.
    ///
    /// GRO may coalesce several datagrams of one flow into a single receive; they are split by
    /// `stride` and routed individually, since each is a separate packet for the connection.
    async fn read_loop(mux: Weak<Self>) {
        let Some(this) = mux.upgrade() else { return };
        let socket = Arc::clone(&this.socket);
        let mut buf = vec![0u8; gro_recv_buf_len(socket.max_gro_segments())];
        // Created once: the listener observes `close()` from here on, and re-polled each
        // iteration. (Re-creating it per packet would allocate per packet.)
        let mut closed = Box::pin(this.closed_notify.listen().fuse());
        if this.closed.load(Ordering::Acquire) {
            // close() ran before we started listening; the notification below is not
            // guaranteed to be observed a second time.
            return;
        }
        drop(this);

        loop {
            let mut meta = [RecvMeta::default(); 1];
            let mut recv = futures::future::poll_fn(|cx| {
                let mut bufs = [IoSliceMut::new(buf.as_mut_slice())];
                socket.poll_recv(cx, &mut bufs, &mut meta)
            })
            .fuse();

            let recv = futures::select! {
                res = recv => res,
                // The mux was closed while the receive was parked.
                _ = closed.as_mut() => return,
            };

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
                        this.route(&buf[off..end], meta.addr, meta.dst_ip, meta.ecn);
                        off = end;
                    }
                }
                Err(err) if is_retryable_socket_recv_error(&err) => continue,
                Err(err) => {
                    warn!("udp_mux: socket read failed, closing the mux: {err}");
                    // Hang up every conn so its driver fails instead of waiting forever on a
                    // socket nobody reads, and refuse new registrations.
                    UDPMux::close(&*this);
                    return;
                }
            }
        }
    }

    /// Route one datagram to its connection.
    ///
    /// A genuine STUN check carries its destination's identity (the USERNAME's local half) and
    /// wins over the learnt-address table: a client that reconnects from the same address with
    /// fresh credentials must reach its *new* connection, not the stale route. Everything else
    /// — DTLS, SRTP, SRTCP, STUN without a resolvable or verifiable USERNAME — routes by the
    /// address learnt when the connection last sent to or was matched for that peer.
    fn route(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        dst_ip: Option<IpAddr>,
        ecn: Option<crate::runtime::EcnCodepoint>,
    ) {
        // The v6 dual-stack view of an IPv4 peer is the v4-mapped address; connections and
        // ICE candidates both speak plain IPv4, so routes are keyed and delivered in the
        // mapped-back form.
        let peer_addr = normalize_inbound_addr(peer_addr);

        if let Some(conn) = self.authenticate_binding_request(data) {
            // An authenticated check (re)points the address route at the connection it names.
            self.learn_route(peer_addr, &conn);
            conn.deliver(data, peer_addr, dst_ip, ecn);
            return;
        }

        if let Some(conn) = self
            .address_map
            .lock()
            .unwrap()
            .get(&peer_addr)
            .and_then(Weak::upgrade)
        {
            conn.deliver(data, peer_addr, dst_ip, ecn);
            return;
        }

        trace!(
            "udp_mux: dropping {} bytes from unknown {peer_addr}",
            data.len()
        );
    }

    /// The connection named by a genuine ICE connectivity check, or `None`.
    ///
    /// Only a `BINDING_REQUEST` whose USERNAME names a registered ufrag **and** whose
    /// `MESSAGE-INTEGRITY` verifies against that ufrag's password may claim a route. This is
    /// the authentication boundary the route table is missing without: a peer that floods
    /// checks from many spoofed source addresses would otherwise grow the address map, and one
    /// that knows another connection's ufrag could re-point its route.
    fn authenticate_binding_request(&self, data: &[u8]) -> Option<Arc<UdpMuxedConn>> {
        if !is_stun_message(data) {
            return None;
        }
        let mut msg = StunMessage::new();
        msg.unmarshal_binary(data).ok()?;
        if msg.typ != BINDING_REQUEST {
            return None;
        }
        let ufrag = stun_username(&msg)?;
        let conn = self.conns.lock().unwrap().get(&ufrag).cloned()?;
        // Rate-limit before the HMAC: a peer that knows its ufrag can otherwise make the reader
        // compute one HMAC per datagram forever.
        if !conn.check_rate.lock().unwrap().allow(Instant::now()) {
            return None;
        }
        let pwd = conn.credentials.lock().unwrap().get(&ufrag).cloned()?;
        MessageIntegrity::check(&mut msg, pwd.as_bytes(), self.crypto.crypto()).ok()?;
        Some(conn)
    }

    /// Point the peer-address route at `conn`, evicting the connection's oldest learnt address
    /// when it has reached [`MAX_LEARNED_ADDRS_PER_CONN`].
    fn learn_route(&self, peer_addr: SocketAddr, conn: &Arc<UdpMuxedConn>) {
        let evicted = conn.learned.lock().unwrap().insert(peer_addr);
        let mut map = self.address_map.lock().unwrap();
        map.insert(peer_addr, Arc::downgrade(conn));
        // If a bounded insert evicted an older address, and it still named this connection,
        // remove its route so the address map cannot outgrow the bound either.
        if let Some(evicted) = evicted
            && map
                .get(&evicted)
                .and_then(Weak::upgrade)
                .is_some_and(|owner| Arc::ptr_eq(&owner, conn))
        {
            map.remove(&evicted);
        }
    }
}

impl UDPMux for UDPMuxDefault {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn register_conn(&self) -> io::Result<(UDPMuxConnHandle, Arc<dyn AsyncUdpSocket>)> {
        // The closed check must hold the lock: close() sets the flag *before* draining the
        // table, so a registration that checked the flag unlocked could slip in behind the
        // drain and sit in a dead mux, never routed to and never hung up.
        if self.closed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "udp mux is closed",
            ));
        }

        let handle = UDPMuxConnHandle(self.next_handle.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = async_channel::bounded(MUXED_CONN_CHANNEL_CAPACITY);
        let conn = Arc::new(UdpMuxedConn {
            handle,
            local_addr: self.local_addr,
            socket: Arc::clone(&self.socket),
            address_map: Arc::clone(&self.address_map),
            pool: Arc::clone(&self.pool),
            rx: Mutex::new(Box::pin(rx)),
            tx,
            // Empty: the driver adds the connection's ufrag(s) via `add_ufrag` once it knows
            // the credentials (from the setting engine, or from the local SDP after the first
            // `set_local_description`).
            credentials: Mutex::new(HashMap::new()),
            learned: Mutex::new(LearnedAddrs::default()),
            dropped: AtomicU64::new(0),
            check_rate: Mutex::new(CheckRateLimiter::new()),
            me: OnceLock::new(),
        });
        let _ = conn.me.set(Arc::downgrade(&conn));
        self.conns_by_handle
            .lock()
            .unwrap()
            .insert(handle, conn.clone());
        Ok((handle, conn))
    }

    fn add_ufrag(&self, handle: UDPMuxConnHandle, ufrag: &str, pwd: &str) -> io::Result<()> {
        validate_ufrag(ufrag)?;
        let mut conns = self.conns.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "udp mux is closed",
            ));
        }
        if conns.contains_key(ufrag) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("ICE ufrag {ufrag:?} is already registered on the UDP mux"),
            ));
        }
        let conn = self
            .conns_by_handle
            .lock()
            .unwrap()
            .get(&handle)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unknown udp mux connection handle",
                )
            })?;
        conn.credentials
            .lock()
            .unwrap()
            .insert(ufrag.to_owned(), pwd.to_owned());
        conns.insert(ufrag.to_owned(), conn);
        Ok(())
    }

    fn remove_ufrag(&self, ufrag: &str) {
        let conn = self.conns.lock().unwrap().remove(ufrag);
        if let Some(conn) = conn {
            conn.credentials.lock().unwrap().remove(ufrag);
        }
    }

    fn remove_conn(&self, handle: UDPMuxConnHandle) {
        let removed = self.conns_by_handle.lock().unwrap().remove(&handle);
        if let Some(conn) = removed {
            // Drop queued datagrams and wake any poll of the connection.
            conn.close();
            // Drop every ufrag that named this connection.
            self.conns
                .lock()
                .unwrap()
                .retain(|_, c| !Arc::ptr_eq(c, &conn));
            // Purge its peer-address routes. Entries are matched by identity, not address
            // value, so a route a *newer* connection has since taken over is left alone.
            let mut purged = Vec::new();
            self.address_map.lock().unwrap().retain(|addr, route| {
                let keep = route.upgrade().is_none_or(|c| !Arc::ptr_eq(&c, &conn));
                if !keep {
                    purged.push(*addr);
                }
                keep
            });
            // A surviving connection that learnt one of the purged addresses earlier must
            // re-register it on its next send — its set saying "already claimed" would
            // otherwise keep the route absent.
            if !purged.is_empty() {
                for conn in self.conns_by_handle.lock().unwrap().values() {
                    let mut learned = conn.learned.lock().unwrap();
                    for addr in &purged {
                        learned.remove(addr);
                    }
                }
            }
        }
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        // Drain by handle, not by ufrag: a connection whose ufrag(s) were removed (or never
        // added yet) exists only in the handle table, and the ufrag table may name one
        // connection several times (ICE-restart grace).
        for (_, conn) in self.conns_by_handle.lock().unwrap().drain() {
            conn.close();
        }
        self.conns.lock().unwrap().clear();
        self.address_map.lock().unwrap().clear();
        // The reader parks in the socket's poll_recv; wake it so the task actually ends.
        self.closed_notify.notify_waiters();
    }

    fn dropped_datagrams(&self, handle: UDPMuxConnHandle) -> u64 {
        self.conns_by_handle
            .lock()
            .unwrap()
            .get(&handle)
            .map_or(0, |conn| conn.dropped.load(Ordering::Relaxed))
    }
}

/// ICE username fragments must be 4-256 `ice-char`s (RFC 8445 §5.2.2): ALPHA / DIGIT / '+' /
/// '/'. The mux routes by the first ':'-separated segment of a STUN USERNAME, so a ufrag with
/// anything outside the charset could never match — reject it at registration instead of
/// silently stranding the connection.
pub(crate) fn validate_ufrag(ufrag: &str) -> io::Result<()> {
    let valid = (4..=256).contains(&ufrag.len())
        && ufrag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/');
    if valid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid ICE ufrag {ufrag:?}: must be 4-256 ice-chars (ALPHA/DIGIT/+/)"),
        ))
    }
}

/// The local half of a STUN message's USERNAME — the receiving connection's ufrag.
///
/// `None` for a message with no UTF-8 USERNAME: routing must never guess, so undecodable
/// traffic simply goes nowhere.
pub(crate) fn stun_username(msg: &StunMessage) -> Option<String> {
    let username = msg.get(ATTR_USERNAME).ok()?;
    let username = std::str::from_utf8(&username).ok()?;
    // "local-ufrag:remote-ufrag" from the sender's perspective; the local half names us.
    // ice-char (RFC 8445) excludes ':', so the first segment is the whole local ufrag.
    username.split(':').next().map(str::to_owned)
}

/// The v6 dual-stack view of an IPv4 peer is the v4-mapped address; connections and ICE
/// candidates both speak plain IPv4, so routes are keyed and delivered in the mapped-back form.
fn normalize_inbound_addr(addr: SocketAddr) -> SocketAddr {
    match addr {
        SocketAddr::V6(v6) => match v6.ip().to_ipv4_mapped() {
            Some(v4) => SocketAddr::new(IpAddr::V4(v4), v6.port()),
            None => addr,
        },
        v4 => v4,
    }
}

/// The peer addresses a connection has learnt, with a bound and FIFO eviction.
///
/// The set answers `contains` without allocation; the `order` deque records insertion order so
/// the oldest learnt address is evicted at [`MAX_LEARNED_ADDRS_PER_CONN`]. Eviction is what
/// keeps the global address map bounded even against a peer that sends from many addresses.
#[derive(Default)]
struct LearnedAddrs {
    set: HashSet<SocketAddr>,
    order: VecDeque<SocketAddr>,
}

impl LearnedAddrs {
    fn contains(&self, addr: &SocketAddr) -> bool {
        self.set.contains(addr)
    }

    fn remove(&mut self, addr: &SocketAddr) {
        if self.set.remove(addr) {
            self.order.retain(|a| a != addr);
        }
    }

    /// Inserts `addr`, returning the evicted oldest address when the set was at capacity.
    fn insert(&mut self, addr: SocketAddr) -> Option<SocketAddr> {
        if self.set.insert(addr) {
            self.order.push_back(addr);
            if self.order.len() > MAX_LEARNED_ADDRS_PER_CONN {
                let oldest = self.order.pop_front()?;
                self.set.remove(&oldest);
                return Some(oldest);
            }
        }
        None
    }
}

/// A connection's view of the shared socket: sends go straight out, receives arrive through
/// the mux's routing. Learning a peer's address happens here on send — a connection's first
/// outbound datagram to a peer is what lets later non-STUN traffic from it route back.
struct UdpMuxedConn {
    handle: UDPMuxConnHandle,
    local_addr: SocketAddr,
    socket: Arc<dyn AsyncUdpSocket>,
    address_map: Arc<Mutex<HashMap<SocketAddr, Weak<UdpMuxedConn>>>>,
    pool: Arc<BufPool>,
    /// `&self` polling needs the receiver behind a lock; async-channel's waker registration
    /// survives the guard's drop, so a `Pending` here still wakes the poller. Boxed and pinned
    /// because `async_channel::Receiver` is `!Unpin`.
    rx: Mutex<Pin<Box<async_channel::Receiver<MuxedPacket>>>>,
    /// Kept so `close` can hang up the queue (dropping it closes the channel).
    tx: async_channel::Sender<MuxedPacket>,
    /// The connection's ICE credentials by ufrag. The reader verifies a check's
    /// `MESSAGE-INTEGRITY` against the password of the ufrag it names; on an ICE restart a
    /// second entry is added while the previous one is within its grace period.
    credentials: Mutex<HashMap<String, String>>,
    /// Peer addresses this connection has already claimed in the mux's address map. The
    /// steady-state send path checks only this (uncontended) set; the shared map is locked
    /// solely when a route actually changes. Bounded — see [`LearnedAddrs`].
    learned: Mutex<LearnedAddrs>,
    /// Inbound datagrams this connection dropped because its queue was full (or already closed).
    /// Counted at the drop site so an operator can see mux-level loss instead of a bare trace.
    dropped: AtomicU64,
    /// Rate-limits the STUN check HMAC this connection's traffic can drive in the reader.
    check_rate: Mutex<CheckRateLimiter>,
    /// Self-reference for registering peer-address routes on send, installed by
    /// [`UDPMuxDefault::register_conn`] once the `Arc` exists.
    me: OnceLock<Weak<UdpMuxedConn>>,
}

impl fmt::Debug for UdpMuxedConn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdpMuxedConn")
            .field("handle", &self.handle)
            .field("local_addr", &self.local_addr)
            .finish()
    }
}

impl UdpMuxedConn {
    fn deliver(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        dst_ip: Option<IpAddr>,
        ecn: Option<crate::runtime::EcnCodepoint>,
    ) {
        // Pooled buffers are POOL_BUF_LEN; a bigger datagram gets its own allocation
        // (`BufPool::put` never pools it back) rather than being truncated.
        let mut buf = if data.len() > POOL_BUF_LEN {
            vec![0u8; data.len()].into_boxed_slice()
        } else {
            self.pool.get()
        };
        buf[..data.len()].copy_from_slice(data);
        let packet = MuxedPacket {
            data: buf,
            len: data.len(),
            peer_addr,
            dst_ip,
            ecn,
        };
        match self.tx.try_send(packet) {
            Ok(()) => {}
            Err(async_channel::TrySendError::Full(packet)) => {
                trace!(
                    "udp_mux: connection {:?} is not keeping up, dropping datagram from {peer_addr}",
                    self.handle
                );
                self.dropped.fetch_add(1, Ordering::Relaxed);
                self.pool.put(packet.data);
            }
            Err(async_channel::TrySendError::Closed(packet)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                self.pool.put(packet.data);
            }
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
        if self.tx.is_closed() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "udp mux connection is closed",
            )));
        }
        // Learn the route on first contact with a peer, so a response that beats our return
        // already knows where to go. Steady-state sends only touch the per-connection set —
        // the shared map is written when routes change, not per packet.
        let dest = normalize_inbound_addr(transmit.destination);
        let mut learned = self.learned.lock().unwrap();
        if !learned.contains(&dest) {
            let evicted = learned.insert(dest);
            if let Some(me) = self.me.get() {
                let mut address_map = self.address_map.lock().unwrap();
                // Claim only an unowned route. A send goes wherever the remote's candidates
                // say, so it must not move a live connection's route: only a STUN check
                // naming our ufrag may (see `route`). If the owner is removed later,
                // `remove_conn` clears the address from every learned set and the next send
                // claims it.
                let owned_by_other = address_map
                    .get(&dest)
                    .and_then(Weak::upgrade)
                    .is_some_and(|owner| !std::ptr::eq(Arc::as_ptr(&owner), self));
                if !owned_by_other {
                    address_map.insert(dest, me.clone());
                }
                // A bounded learned insert may have evicted an older address; drop its route
                // if it still names this connection.
                if let Some(evicted) = evicted
                    && address_map
                        .get(&evicted)
                        .and_then(Weak::upgrade)
                        .is_some_and(|owner| std::ptr::eq(Arc::as_ptr(&owner), self))
                {
                    address_map.remove(&evicted);
                }
            }
        }
        drop(learned);
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
                if packet.len > bufs[0].len() {
                    warn!(
                        "udp_mux: truncating {}-byte datagram from {} to {} bytes",
                        packet.len,
                        packet.peer_addr,
                        bufs[0].len()
                    );
                }
                let n = packet.len.min(bufs[0].len());
                bufs[0][..n].copy_from_slice(&packet.data[..n]);
                let m = &mut meta[0];
                m.addr = packet.peer_addr;
                m.len = n;
                // One datagram per read, so the buffer's stride is its length; GRO splitting
                // happened in the mux's reader already.
                m.stride = n.max(1);
                m.ecn = packet.ecn;
                m.dst_ip = packet.dst_ip;
                self.pool.put(packet.data);
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

    /// The mux's reader de-segments GRO before this connection ever sees a datagram, so the
    /// driver only needs a single-datagram receive buffer. Reporting `1` (rather than the
    /// shared socket's GRO figure) keeps the driver's per-connection buffers at the plain
    /// ~2 KB instead of the ~96 KB a GRO-capable socket would warrant — an order of magnitude
    /// saved per connection at SFU scale. WebRTC keeps its datagrams at or below the wire MTU,
    /// so the plain single-datagram buffer is enough.
    fn max_gro_segments(&self) -> usize {
        1
    }
}

#[cfg(test)]
mod tests {
    //! Routing and lifecycle tests, driven synchronously: `route` is called by hand rather
    //! than through the reader task, so no runtime is needed. The shared socket is a fake;
    //! what is under test is the demux table, not I/O.
    use super::*;
    use crate::runtime::poll_once;
    use rtc::stun::message::{Setter, TransactionId};
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
            conns_by_handle: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(0),
            address_map: Arc::new(Mutex::new(HashMap::new())),
            pool: Arc::new(BufPool {
                free: Mutex::new(Vec::new()),
            }),
            crypto: crypto::default_provider().unwrap(),
            closed_notify: Arc::new(Notify::new()),
            closed: AtomicBool::new(false),
        })
    }

    /// A `BINDING_REQUEST` with USERNAME `username` authenticated with ICE password `pwd`.
    fn binding_request(username: &str, pwd: &str) -> Vec<u8> {
        let crypto = crypto::default_provider().unwrap();
        let mut message = StunMessage::new();
        message
            .build(&[
                Box::new(BINDING_REQUEST),
                Box::<TransactionId>::default(),
                Box::new(Username::new(ATTR_USERNAME, username.to_owned())),
            ])
            .unwrap();
        let integrity = MessageIntegrity::new_short_term_integrity_with_provider(
            pwd.to_owned(),
            crypto.crypto(),
        );
        integrity.add_to(&mut message).unwrap();
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

    /// Register a connection and add its first ufrag, the way the driver does once it knows
    /// the connection's credentials.
    fn register(
        mux: &Arc<UDPMuxDefault>,
        ufrag: &str,
        pwd: &str,
    ) -> (UDPMuxConnHandle, Arc<dyn AsyncUdpSocket>) {
        let (handle, conn) = mux.register_conn().unwrap();
        mux.add_ufrag(handle, ufrag, pwd).unwrap();
        (handle, conn)
    }

    #[test]
    fn routes_a_stun_check_to_the_conn_named_by_the_username() {
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");
        let (_, conn_b) = register(&mux, "bbbb", "pwd-bbb");

        // USERNAME is "local-ufrag:remote-ufrag" from the sender's perspective: the first
        // half names the *receiving* connection.
        let packet = binding_request("aaaa:remote", "pwd-aaa");
        mux.route(&packet, peer(), Some(IpAddr::from([127, 0, 0, 1])), None);

        assert_eq!(poll_packet(&conn_b), None, "conn b must see nothing");
        let (data, from, dst_ip) = poll_packet(&conn_a).expect("conn a gets the check");
        assert_eq!(data, packet);
        assert_eq!(from, peer());
        assert_eq!(dst_ip, Some(IpAddr::from([127, 0, 0, 1])));

        // Routing by ufrag also learns the peer address: the next, ufrag-less datagram from
        // the same peer goes straight to the connection without parsing.
        mux.route(b"\x80\x77dtls-not-stun", peer(), None, None);
        let (data, _, _) = poll_packet(&conn_a).expect("conn a gets the follow-up");
        assert_eq!(data, b"\x80\x77dtls-not-stun");
        assert_eq!(poll_packet(&conn_b), None);
    }

    #[test]
    fn routes_by_the_address_learnt_on_send() {
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");
        let (_, conn_b) = register(&mux, "bbbb", "pwd-bbb");

        // The connection's first outbound datagram claims the peer address; DTLS/SRTP from
        // that peer — never carrying a ufrag — routes back to it.
        send_to(&conn_b, b"binding response", peer());
        mux.route(b"\x80\x77dtls-not-stun", peer(), None, None);

        let (data, _, _) = poll_packet(&conn_b).expect("conn b gets the reply");
        assert_eq!(data, b"\x80\x77dtls-not-stun");
        assert_eq!(poll_packet(&conn_a), None);
    }

    #[test]
    fn drops_packets_from_unknown_peers() {
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");

        mux.route(b"\x80\x77dtls-not-stun", peer(), None, None);
        // A STUN check naming a ufrag nobody registered.
        mux.route(
            &binding_request("zzzz:remote", "pwd-zzz"),
            peer(),
            None,
            None,
        );
        // Garbage that is not STUN at all.
        mux.route(
            &binding_request("aaaa:remote", "pwd-aaa")[..10],
            peer(),
            None,
            None,
        );

        assert_eq!(poll_packet(&conn_a), None, "nothing may be routed");
    }

    #[test]
    fn a_stun_check_without_integrity_is_ignored() {
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");

        // A well-formed BINDING_REQUEST naming a real ufrag, but with no MESSAGE-INTEGRITY.
        let mut message = StunMessage::new();
        message
            .build(&[
                Box::new(BINDING_REQUEST),
                Box::<TransactionId>::default(),
                Box::new(Username::new(ATTR_USERNAME, "aaaa:remote".to_owned())),
            ])
            .unwrap();

        mux.route(&message.raw, peer(), None, None);
        assert_eq!(
            poll_packet(&conn_a),
            None,
            "an unauthenticated check is dropped"
        );
        assert!(
            mux.address_map.lock().unwrap().is_empty(),
            "an unauthenticated check must not learn a route"
        );
    }

    #[test]
    fn a_stun_check_with_wrong_integrity_is_ignored() {
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");

        // A valid BINDING_REQUEST, but signed with the wrong password.
        let packet = binding_request("aaaa:remote", "wrong-pwd");
        mux.route(&packet, peer(), None, None);
        assert_eq!(
            poll_packet(&conn_a),
            None,
            "a check with a bad MAC is dropped"
        );
        assert!(
            mux.address_map.lock().unwrap().is_empty(),
            "a check with a bad MAC must not learn a route"
        );
    }

    #[test]
    fn a_binding_response_does_not_learn_a_route() {
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");

        // A response (even one carrying a USERNAME) is not a check: it must not claim a route.
        let mut message = StunMessage::new();
        message.typ = rtc::stun::message::BINDING_SUCCESS;
        message
            .build(&[
                Box::new(rtc::stun::message::BINDING_SUCCESS),
                Box::<TransactionId>::default(),
                Box::new(Username::new(ATTR_USERNAME, "aaaa:remote".to_owned())),
            ])
            .unwrap();
        let crypto = crypto::default_provider().unwrap();
        let integrity = MessageIntegrity::new_short_term_integrity_with_provider(
            "pwd-aaa".to_owned(),
            crypto.crypto(),
        );
        integrity.add_to(&mut message).unwrap();

        mux.route(&message.raw, peer(), None, None);
        assert_eq!(poll_packet(&conn_a), None);
        assert!(mux.address_map.lock().unwrap().is_empty());
    }

    /// A datagram bigger than the pooled buffer size gets its own allocation and arrives
    /// whole — the pool must not become a silent truncation point.
    #[test]
    fn an_oversized_datagram_is_delivered_untruncated() {
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");

        let datagram = vec![0x5au8; POOL_BUF_LEN * 2];
        mux.route(
            &binding_request("aaaa:remote", "pwd-aaa"),
            peer(),
            None,
            None,
        );
        let _ = poll_packet(&conn_a);
        mux.route(&datagram, peer(), None, None);

        let mut buf = vec![0u8; POOL_BUF_LEN * 2 + 64];
        let mut meta = [RecvMeta::default(); 1];
        let received = poll_once(|cx| {
            let mut bufs = [IoSliceMut::new(buf.as_mut_slice())];
            conn_a.poll_recv(cx, &mut bufs, &mut meta)
        })
        .and_then(|result| result.ok())
        .map(|_| buf[..meta[0].len].to_vec());
        assert_eq!(received, Some(datagram));
    }

    /// Dropping the last mux handle is an implicit close: every conn's queue hangs up, so a
    /// parked `poll_recv` reports the socket gone instead of waiting forever.
    #[test]
    fn dropping_the_mux_hangs_up_its_conns() {
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");
        drop(mux);

        let mut buf = [0u8; 64];
        let mut meta = [RecvMeta::default(); 1];
        let result = poll_once(|cx| {
            let mut bufs = [IoSliceMut::new(&mut buf)];
            conn_a.poll_recv(cx, &mut bufs, &mut meta)
        });
        assert_eq!(
            result.and_then(|r| r.err()).map(|e| e.kind()),
            Some(io::ErrorKind::BrokenPipe),
            "a dropped mux's conns report the socket gone"
        );
    }

    #[test]
    fn a_new_claimant_of_a_peer_address_wins_the_route() {
        // A client that reconnects from the same address with fresh credentials must not be
        // stranded on its previous connection's route: the ufrag match re-points it.
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");
        let (_, conn_b) = register(&mux, "bbbb", "pwd-bbb");

        send_to(&conn_a, b"outbound", peer());
        mux.route(
            &binding_request("bbbb:remote", "pwd-bbb"),
            peer(),
            None,
            None,
        );

        // The route has moved: the check itself reached the new connection ...
        let (check, _, _) = poll_packet(&conn_b).expect("conn b gets its own check");
        assert_eq!(check, binding_request("bbbb:remote", "pwd-bbb"));
        // ... and media from the peer now goes to it as well.
        mux.route(b"\x80\x77dtls-not-stun", peer(), None, None);
        let (data, _, _) = poll_packet(&conn_b).expect("conn b takes over the peer");
        assert_eq!(data, b"\x80\x77dtls-not-stun");
        assert_eq!(poll_packet(&conn_a), None, "the stale route is gone");
    }

    #[test]
    fn a_send_does_not_take_over_a_live_connections_route() {
        // A send goes wherever the remote's candidates say, so it must not move a route a
        // live connection owns (a peer could list another client's address and pull its
        // DTLS/SRTP). Only a STUN check naming our ufrag moves an owned route.
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");
        let (_, conn_b) = register(&mux, "bbbb", "pwd-bbb");

        send_to(&conn_a, b"outbound", peer());
        send_to(&conn_b, b"outbound", peer()); // must NOT steal by sending

        mux.route(b"\x80\x77dtls-not-stun", peer(), None, None);
        let (data, _, _) = poll_packet(&conn_a).expect("the route still belongs to conn a");
        assert_eq!(data, b"\x80\x77dtls-not-stun");
        assert_eq!(poll_packet(&conn_b), None);

        // Only a STUN check naming b's ufrag may move it.
        mux.route(
            &binding_request("bbbb:remote", "pwd-bbb"),
            peer(),
            None,
            None,
        );
        let (check, _, _) = poll_packet(&conn_b).expect("conn b gets its own check");
        assert_eq!(check, binding_request("bbbb:remote", "pwd-bbb"));
        mux.route(b"\x80\x77dtls-not-stun", peer(), None, None);
        let (data, _, _) = poll_packet(&conn_b).expect("conn b's check moved the route");
        assert_eq!(data, b"\x80\x77dtls-not-stun");
    }

    #[test]
    fn add_ufrag_refuses_a_duplicate_ufrag() {
        let mux = mux();
        register(&mux, "aaaa", "pwd-aaa");
        let (handle_b, _) = mux.register_conn().unwrap();
        let dup = mux.add_ufrag(handle_b, "aaaa", "pwd-bbb");
        assert_eq!(dup.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn add_ufrag_routes_under_both_ufrags_to_one_conn() {
        let mux = mux();
        let (handle, conn) = register(&mux, "aaaa", "pwd-aaa");

        // An ICE restart rotates the credentials; the old ufrag stays until its grace ends.
        mux.add_ufrag(handle, "bbbb", "pwd-bbb").unwrap();

        let new_check = binding_request("bbbb:remote", "pwd-bbb");
        mux.route(&new_check, peer(), None, None);
        let (data, _, _) = poll_packet(&conn).expect("conn receives the new-ufrag check");
        assert_eq!(data, new_check);

        // The old ufrag still routes during the grace period.
        let old_check = binding_request("aaaa:remote", "pwd-aaa");
        mux.route(&old_check, peer(), None, None);
        let (data, _, _) = poll_packet(&conn).expect("conn receives the old-ufrag check");
        assert_eq!(data, old_check);

        // Removing one ufrag stops that half only; the other still routes. Route from a fresh
        // peer address so the learnt-address table cannot mask the result.
        mux.remove_ufrag("aaaa");
        let other_peer = SocketAddr::from(([192, 0, 2, 9], 50000));
        mux.route(
            &binding_request("aaaa:remote", "pwd-aaa"),
            other_peer,
            None,
            None,
        );
        assert_eq!(poll_packet(&conn), None, "removed ufrag no longer routes");
        mux.route(
            &binding_request("bbbb:remote", "pwd-bbb"),
            other_peer,
            None,
            None,
        );
        assert!(poll_packet(&conn).is_some(), "surviving ufrag still routes");
    }

    #[test]
    fn remove_conn_by_handle_does_not_evict_a_reused_ufrag() {
        let mux = mux();
        let (handle_a, conn_a) = register(&mux, "aaaa", "pwd-aaa");

        // The old connection is deregistered (its driver's Drop) and its ufrag reused.
        mux.remove_conn(handle_a);
        let (_, conn_b) = register(&mux, "aaaa", "pwd-bbb");

        // A belated removal of the *old* handle must not touch the new connection.
        mux.remove_conn(handle_a);

        mux.route(
            &binding_request("aaaa:remote", "pwd-bbb"),
            peer(),
            None,
            None,
        );
        assert!(
            poll_packet(&conn_b).is_some(),
            "the reused ufrag still routes"
        );
        assert_eq!(poll_packet(&conn_a), None, "the old conn no longer routes");
    }

    #[test]
    fn remove_conn_frees_the_ufrag_and_its_routes() {
        let mux = mux();
        let (handle, conn_a) = register(&mux, "aaaa", "pwd-aaa");
        send_to(&conn_a, b"outbound", peer());

        mux.remove_conn(handle);
        let (new_handle, _) = mux.register_conn().unwrap();
        assert!(
            mux.add_ufrag(new_handle, "aaaa", "pwd-aaa").is_ok(),
            "the ufrag is free again"
        );

        // The freed connection's route is gone with it: nothing routes anywhere now.
        mux.route(b"\x80\x77dtls-not-stun", peer(), None, None);
        assert_eq!(poll_packet(&conn_a), None);
    }

    #[test]
    fn remove_conn_keeps_routes_a_new_conn_has_taken_over() {
        let mux = mux();
        let (handle_a, conn_a) = register(&mux, "aaaa", "pwd-aaa");
        let (_, conn_b) = register(&mux, "bbbb", "pwd-bbb");

        send_to(&conn_a, b"outbound", peer());
        // conn b takes the peer over with its STUN check, then conn a is removed:
        // conn b's route must survive the purge.
        mux.route(
            &binding_request("bbbb:remote", "pwd-bbb"),
            peer(),
            None,
            None,
        );
        let _ = poll_packet(&conn_b);
        mux.remove_conn(handle_a);

        mux.route(b"\x80\x77dtls-not-stun", peer(), None, None);
        let (data, _, _) = poll_packet(&conn_b).expect("conn b keeps its route");
        assert_eq!(data, b"\x80\x77dtls-not-stun");
    }

    #[test]
    fn remove_conn_lets_a_surviving_previous_owner_reclaim_the_route() {
        // A owns the route, B takes it over, B is removed: the purge must clear the address
        // from A's learned set too, or A's next send would skip re-registering a route that
        // no longer exists.
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");
        let (handle_b, conn_b) = register(&mux, "bbbb", "pwd-bbb");

        send_to(&conn_a, b"outbound", peer());
        mux.route(
            &binding_request("bbbb:remote", "pwd-bbb"),
            peer(),
            None,
            None,
        ); // b takes over
        let _ = poll_packet(&conn_b);
        mux.remove_conn(handle_b);

        // A's next send re-claims the route ...
        send_to(&conn_a, b"outbound again", peer());
        // ... so peer traffic routes to it again.
        mux.route(b"\x80\x77dtls-not-stun", peer(), None, None);
        let (data, _, _) = poll_packet(&conn_a).expect("conn a reclaims the route");
        assert_eq!(data, b"\x80\x77dtls-not-stun");
    }

    #[test]
    fn learned_addresses_are_bounded() {
        let mux = mux();
        let (_, conn) = register(&mux, "aaaa", "pwd-aaa");

        for i in 0..(MAX_LEARNED_ADDRS_PER_CONN + 16) {
            let addr = SocketAddr::from(([192, 0, 2, (i % 250) as u8], 40000 + i as u16));
            send_to(&conn, b"outbound", addr);
        }

        let map_len = mux.address_map.lock().unwrap().len();
        assert!(
            map_len <= MAX_LEARNED_ADDRS_PER_CONN,
            "the address map stays within the per-connection bound"
        );
        assert!(map_len > 0, "routes are still learnt below the bound");
    }

    #[test]
    fn close_hangs_up_every_conn_and_rejects_registrations() {
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");
        let (_, conn_b) = register(&mux, "bbbb", "pwd-bbb");

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
        assert!(mux.register_conn().is_err());
    }

    #[test]
    fn close_clears_all_peer_address_routes() {
        let mux = mux();
        let (_, conn) = register(&mux, "aaaa", "pwd-aaa");
        send_to(&conn, b"outbound", peer());
        assert_eq!(mux.address_map.lock().unwrap().len(), 1);

        mux.close();

        assert!(mux.address_map.lock().unwrap().is_empty());
        let transmit = Transmit {
            destination: peer(),
            ecn: None,
            contents: b"after close",
            segment_size: None,
            src_ip: None,
        };
        let result = poll_once(|cx| conn.poll_send(cx, &transmit));
        assert_eq!(
            result.and_then(|r| r.err()).map(|e| e.kind()),
            Some(io::ErrorKind::BrokenPipe)
        );
        assert!(mux.address_map.lock().unwrap().is_empty());
    }

    #[test]
    fn v4_mapped_v6_peer_addresses_normalize_to_v4_routes() {
        // A dual-stack mux socket reports IPv4 peers as v4-mapped IPv6; candidates and
        // routes both speak plain IPv4, so the mapping must be undone on the way in.
        let mux = mux();
        let (_, conn_a) = register(&mux, "aaaa", "pwd-aaa");
        let v6_mapped: SocketAddr = "[::ffff:192.0.2.1]:40000".parse().unwrap();

        mux.route(
            &binding_request("aaaa:remote", "pwd-aaa"),
            v6_mapped,
            None,
            None,
        );
        let (_, from, _) = poll_packet(&conn_a).expect("conn a gets the check");
        assert_eq!(from, peer(), "the conn sees the plain IPv4 address");

        // And the learnt route matches the plain form too.
        mux.route(b"\x80\x77dtls-not-stun", peer(), None, None);
        assert!(poll_packet(&conn_a).is_some());
    }

    #[test]
    fn stun_username_reads_the_local_half() {
        let mut message = StunMessage::new();
        message
            .build(&[
                Box::new(BINDING_REQUEST),
                Box::<TransactionId>::default(),
                Box::new(Username::new(ATTR_USERNAME, "local:remote".to_owned())),
            ])
            .unwrap();
        assert_eq!(stun_username(&message), Some("local".to_owned()));

        // No USERNAME → nothing to route by.
        let mut no_username = StunMessage::new();
        no_username
            .build(&[Box::new(BINDING_REQUEST), Box::<TransactionId>::default()])
            .unwrap();
        assert_eq!(stun_username(&no_username), None);
    }
}

#[cfg(all(test, feature = "runtime-tokio"))]
mod tokio_tests {
    use super::*;

    /// Dropping the last mux handle must wake the parked reader — the reader holds the
    /// socket, not the mux, so without the wake the port stays bound.
    #[test]
    fn dropping_the_last_handle_releases_the_port() {
        crate::runtime::TokioRuntime.block_on(Box::pin(async {
            let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            let port = socket.local_addr().unwrap().port();
            let mux = UDPMuxDefault::new(Arc::new(crate::runtime::TokioRuntime), socket).unwrap();
            drop(mux);

            for _ in 0..200 {
                if let Ok(sock) = std::net::UdpSocket::bind(("127.0.0.1", port)) {
                    drop(sock);
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            panic!("the port is still held after dropping the mux");
        }));
    }

    /// `close()` wakes the parked reader: once the handle is dropped after the close, the
    /// port is released.
    #[test]
    fn close_then_drop_releases_the_port() {
        crate::runtime::TokioRuntime.block_on(Box::pin(async {
            let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            let port = socket.local_addr().unwrap().port();
            let mux = UDPMuxDefault::new(Arc::new(crate::runtime::TokioRuntime), socket).unwrap();
            mux.close();
            drop(mux);

            for _ in 0..200 {
                if let Ok(sock) = std::net::UdpSocket::bind(("127.0.0.1", port)) {
                    drop(sock);
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            panic!("the port is still held after close() + drop");
        }));
    }
}
