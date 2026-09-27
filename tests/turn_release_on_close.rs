//! Regression test for webrtc#903: `close()` must release the connection's TURN allocations.
//!
//! A TURN allocation is released with a Refresh carrying LIFETIME=0 (RFC 8656 §7). The driver
//! sends those releases on its way out, but on the general runtime `close()` used to abort the
//! driver without waiting, so whether the releases went out was a race — and on master they were
//! never queued at all (webrtc#895 had landed on v0.20.x only). The server kept the allocations
//! until their lifetime expired.
//!
//! The same release is owed when an ICE restart rebinds the sockets: the old allocation belongs to
//! a socket that is about to be dropped, and a release sent from its replacement would not match
//! the allocation's 5-tuple.
//!
//! A mock TURN server counts the releases it receives. Each close case is repeated because the
//! old failure was a race.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rtc::peer_connection::configuration::setting_engine::SettingEngineBuilder;
use rtc::stun::attributes::{ATTR_NONCE, ATTR_REALM};
use rtc::stun::error_code::CODE_UNAUTHORIZED;
use rtc::stun::message::{
    CLASS_ERROR_RESPONSE, CLASS_SUCCESS_RESPONSE, Getter, METHOD_ALLOCATE,
    METHOD_CREATE_PERMISSION, METHOD_REFRESH, Message as StunMessage, MessageType,
};
use rtc::stun::textattrs::{Nonce, Realm};
use rtc::turn::proto::lifetime::Lifetime;
use rtc::turn::proto::relayaddr::RelayedAddress;
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCIceGatheringState, RTCIceServer, RTCIceTransportPolicy,
};
use webrtc::runtime::{AsyncUdpSocket, Mutex, Receiver, Sender, channel};

mod common;
use common::{block_on, runtime, sleep, timeout};

/// Each case is repeated this many times: the old failure was a race on the general runtime.
const RUNS: usize = 5;

/// How long after `close()` returns the release may take to reach the mock server.
const RELEASE_TIMEOUT: Duration = Duration::from_secs(2);

/// What the mock TURN server has seen.
#[derive(Default)]
struct Counts {
    allocations: AtomicUsize,
    releases: AtomicUsize,
}

/// A TURN server that grants every authenticated Allocate and counts Refresh(LIFETIME=0).
async fn run_mock_turn_server(
    socket: Arc<dyn AsyncUdpSocket>,
    relay_addr: SocketAddr,
    counts: Arc<Counts>,
) {
    let mut buf = vec![0u8; 2048];
    loop {
        let Ok((n, peer_addr)) = socket.recv_from(&mut buf).await else {
            break;
        };
        let mut msg = StunMessage::new();
        msg.raw = buf[..n].to_vec();
        if msg.decode().is_err() {
            continue;
        }

        let mut response = StunMessage::new();
        let built = match msg.typ.method {
            METHOD_ALLOCATE if msg.get(ATTR_NONCE).is_err() => response.build(&[
                Box::new(msg.transaction_id),
                Box::new(MessageType::new(METHOD_ALLOCATE, CLASS_ERROR_RESPONSE)),
                Box::new(CODE_UNAUTHORIZED),
                Box::new(Realm::new(ATTR_REALM, "webrtc.rs".to_owned())),
                Box::new(Nonce::new(ATTR_NONCE, "nonce".to_owned())),
            ]),
            METHOD_ALLOCATE => {
                counts.allocations.fetch_add(1, Ordering::SeqCst);
                response.build(&[
                    Box::new(msg.transaction_id),
                    Box::new(MessageType::new(METHOD_ALLOCATE, CLASS_SUCCESS_RESPONSE)),
                    Box::new(RelayedAddress {
                        ip: relay_addr.ip(),
                        port: relay_addr.port(),
                    }),
                    Box::new(Lifetime(Duration::from_secs(600))),
                ])
            }
            METHOD_REFRESH => {
                let mut lifetime = Lifetime::default();
                let release = lifetime.get_from(&msg).is_ok() && lifetime.0.is_zero();
                if release {
                    counts.releases.fetch_add(1, Ordering::SeqCst);
                }
                response.build(&[
                    Box::new(msg.transaction_id),
                    Box::new(MessageType::new(METHOD_REFRESH, CLASS_SUCCESS_RESPONSE)),
                    Box::new(Lifetime(if release {
                        Duration::ZERO
                    } else {
                        Duration::from_secs(600)
                    })),
                ])
            }
            METHOD_CREATE_PERMISSION => response.build(&[
                Box::new(msg.transaction_id),
                Box::new(MessageType::new(
                    METHOD_CREATE_PERMISSION,
                    CLASS_SUCCESS_RESPONSE,
                )),
            ]),
            _ => continue,
        };
        if built.is_err() {
            continue;
        }
        if socket.send_to(&response.raw, peer_addr).await.is_err() {
            break;
        }
    }
}

/// Signals when ICE gathering completes, and optionally closes the connection from inside that
/// callback — which runs on the driver task.
struct Handler {
    gathered: Sender<()>,
    close_in_callback: Mutex<Option<Arc<dyn PeerConnection>>>,
    close_duration: Sender<Duration>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state != RTCIceGatheringState::Complete {
            return;
        }
        let pc = self.close_in_callback.lock().await.take();
        if let Some(pc) = pc {
            let start = Instant::now();
            pc.close().await.expect("close from a callback");
            let _ = self.close_duration.try_send(start.elapsed());
        }
        let _ = self.gathered.try_send(());
    }
}

/// Builds a relay-only connection against a fresh mock TURN server, gathers (which allocates a
/// relay), closes it — from the test or from the gathering callback — and returns how long
/// `close()` took and whether the server then saw every allocation released.
async fn allocate_then_close(dedicated_reactor: bool, close_in_callback: bool) -> (Duration, bool) {
    let runtime = runtime();
    let std_socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind mock TURN server");
    let turn_addr = std_socket.local_addr().expect("mock TURN address");
    let socket = runtime
        .wrap_udp_socket(std_socket)
        .expect("wrap mock TURN socket");
    let counts = Arc::new(Counts::default());
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000);
    let server = runtime.spawn(Box::pin(run_mock_turn_server(
        socket,
        relay_addr,
        counts.clone(),
    )));

    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("register default codecs");
    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec![format!("turn:{turn_addr}?transport=udp")],
            username: "user".to_owned(),
            credential: "pass".to_owned(),
        }])
        .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
        .build();
    let (gathered_tx, mut gathered_rx) = channel(4);
    let (close_duration_tx, mut close_duration_rx) = channel(4);
    let handler = Arc::new(Handler {
        gathered: gathered_tx,
        close_in_callback: Mutex::new(None),
        close_duration: close_duration_tx,
    });
    let mut builder = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_handler(handler.clone())
        .with_udp_addrs(vec!["127.0.0.1:0"]);
    if dedicated_reactor {
        builder = builder.with_dedicated_reactor_pool_size(1);
    }
    let pc: Arc<dyn PeerConnection> =
        Arc::new(builder.build().await.expect("build peer connection"));
    if close_in_callback {
        *handler.close_in_callback.lock().await = Some(Arc::clone(&pc));
    }

    let _ = pc
        .create_data_channel("data", None)
        .await
        .expect("create data channel");
    let offer = pc.create_offer(None).await.expect("create offer");
    pc.set_local_description(offer)
        .await
        .expect("set local description");
    timeout(Duration::from_secs(5), gathered_rx.recv())
        .await
        .expect("relay gathering completes");
    let allocations = counts.allocations.load(Ordering::SeqCst);
    assert!(allocations > 0, "gathering allocated a relay");

    let close_duration = if close_in_callback {
        timeout(Duration::from_secs(10), close_duration_rx.recv())
            .await
            .expect("close() from the callback returns")
            .expect("close duration reported")
    } else {
        let start = Instant::now();
        pc.close().await.expect("close");
        start.elapsed()
    };

    let deadline = Instant::now() + RELEASE_TIMEOUT;
    let mut released = false;
    while Instant::now() < deadline {
        if counts.releases.load(Ordering::SeqCst) >= allocations {
            released = true;
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }

    server.abort();
    (close_duration, released)
}

#[test]
fn close_releases_turn_allocations() {
    block_on(async {
        for dedicated_reactor in [false, true] {
            for run in 0..RUNS {
                let (_, released) = allocate_then_close(dedicated_reactor, false).await;
                assert!(
                    released,
                    "close() left the TURN allocation on the server \
                     (dedicated_reactor={dedicated_reactor}, run {run})"
                );
            }
        }
    });
}

/// `close()` from an application callback runs on the driver task itself. It must neither stall
/// waiting for a driver that cannot finish until the callback returns, nor abort it before it
/// sends the releases.
#[test]
fn close_from_a_callback_releases_turn_allocations_without_stalling() {
    block_on(async {
        for dedicated_reactor in [false, true] {
            for run in 0..RUNS {
                let (took, released) = allocate_then_close(dedicated_reactor, true).await;
                assert!(
                    took < Duration::from_millis(500),
                    "close() from a callback took {took:?} \
                     (dedicated_reactor={dedicated_reactor}, run {run})"
                );
                assert!(
                    released,
                    "close() from a callback left the TURN allocation on the server \
                     (dedicated_reactor={dedicated_reactor}, run {run})"
                );
            }
        }
    });
}

/// An ICE restart that rebinds the sockets must release the allocation owned by the old socket
/// before dropping it, rather than leaving it on the server until it expires.
#[test]
fn ice_restart_rebind_releases_the_previous_allocation() {
    block_on(async {
        let runtime = runtime();
        let std_socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind mock TURN server");
        let turn_addr = std_socket.local_addr().expect("mock TURN address");
        let socket = runtime
            .wrap_udp_socket(std_socket)
            .expect("wrap mock TURN socket");
        let counts = Arc::new(Counts::default());
        let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000);
        let server = runtime.spawn(Box::pin(run_mock_turn_server(
            socket,
            relay_addr,
            counts.clone(),
        )));

        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("register default codecs");
        let config = RTCConfigurationBuilder::new()
            .with_ice_servers(vec![RTCIceServer {
                urls: vec![format!("turn:{turn_addr}?transport=udp")],
                username: "user".to_owned(),
                credential: "pass".to_owned(),
            }])
            .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
            .build();
        let (gathered_tx, mut gathered_rx) = channel(4);
        let (close_duration_tx, _close_duration_rx) = channel(4);
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_setting_engine(
                SettingEngineBuilder::new()
                    .with_discard_local_candidates_during_ice_restart(true)
                    .build(),
            )
            .with_media_engine(media_engine)
            .with_handler(Arc::new(Handler {
                gathered: gathered_tx,
                close_in_callback: Mutex::new(None),
                close_duration: close_duration_tx,
            }))
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .expect("build peer connection");

        let _ = pc
            .create_data_channel("data", None)
            .await
            .expect("create data channel");
        let offer = pc.create_offer(None).await.expect("create offer");
        pc.set_local_description(offer)
            .await
            .expect("set local description");
        timeout(Duration::from_secs(5), gathered_rx.recv())
            .await
            .expect("first gathering completes");
        assert_eq!(1, counts.allocations.load(Ordering::SeqCst));
        assert_eq!(0, counts.releases.load(Ordering::SeqCst));

        pc.restart_ice().await.expect("restart ICE");
        let offer = pc.create_offer(None).await.expect("create restart offer");
        pc.set_local_description(offer)
            .await
            .expect("set restart local description");
        timeout(Duration::from_secs(5), gathered_rx.recv())
            .await
            .expect("restart gathering completes");

        assert_eq!(
            2,
            counts.allocations.load(Ordering::SeqCst),
            "the restart allocated a relay on the new socket"
        );
        assert_eq!(
            1,
            counts.releases.load(Ordering::SeqCst),
            "the rebind released the allocation owned by the old socket"
        );

        pc.close().await.expect("close");
        server.abort();
    });
}

/// Signals when ICE gathering completes, then blocks the driver in that callback for good.
struct StallingHandler {
    gathered: Sender<()>,
    never: Mutex<Receiver<()>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for StallingHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gathered.try_send(());
            // Nothing is ever sent on this channel, so the driver stays in this callback.
            let _ = self.never.lock().await.recv().await;
        }
    }
}

/// Releasing TURN allocations is best effort: `close()` gives the driver only a brief, capped
/// chance to send them. A driver that cannot get there (here, blocked in an application callback
/// that another task is waiting on) must not hold `close()` up.
#[test]
fn close_does_not_wait_long_for_a_stuck_driver() {
    block_on(async {
        for dedicated_reactor in [false, true] {
            let (gathered_tx, mut gathered_rx) = channel(4);
            let (_never_tx, never_rx) = channel(1);
            let mut builder = PeerConnectionBuilder::new()
                .with_handler(Arc::new(StallingHandler {
                    gathered: gathered_tx,
                    never: Mutex::new(never_rx),
                }))
                .with_udp_addrs(vec!["127.0.0.1:0"]);
            if dedicated_reactor {
                builder = builder.with_dedicated_reactor_pool_size(1);
            }
            let pc = builder.build().await.expect("build peer connection");

            let _ = pc
                .create_data_channel("data", None)
                .await
                .expect("create data channel");
            let offer = pc.create_offer(None).await.expect("create offer");
            pc.set_local_description(offer)
                .await
                .expect("set local description");
            timeout(Duration::from_secs(5), gathered_rx.recv())
                .await
                .expect("gathering completes and the driver enters the stalling callback");

            let start = Instant::now();
            pc.close().await.expect("close");
            let took = start.elapsed();
            assert!(
                took < Duration::from_millis(500),
                "close() waited {took:?} for a stuck driver (dedicated_reactor={dedicated_reactor})"
            );
        }
    });
}
