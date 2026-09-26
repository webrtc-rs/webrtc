//! TURN allocations must be released (a Refresh with LIFETIME=0, RFC 8656 §7) when their
//! transport is retired, rather than left on the server until their lifetime expires.
//!
//! An ICE restart that rebinds the sockets retires the old ones: the old allocation belongs to a
//! socket that is about to be dropped, and a release sent from its replacement would not match
//! the allocation's 5-tuple. A mock TURN server counts the releases it receives.
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
use webrtc::runtime::{AsyncUdpSocket, Mutex, Sender, channel};

mod common;
use common::{block_on, runtime, timeout};

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
