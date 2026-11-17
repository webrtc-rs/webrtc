//! A TURN server reached over UDP that is bound and never answers.
//!
//! This is the shape of an isolated network: the address resolves, the packets leave, and
//! nothing comes back. STUN's unreliable schedule has an end — seven requests on a
//! lengthening RTO — and gathering has to end with it, because a non-trickle caller is
//! waiting for `Complete` before it can send its offer at all.
//!
//! It did not. `Client::allocate()` hands back the transaction id of the *authenticated*
//! retry, so [`RTCTurnRelayer`] matches an answer against that id; a transaction that ran
//! out was reported under the id it was stored with, the anonymous one, so the relayer
//! never recognised its own Allocate giving up. `gather_finished` stayed false and
//! `TurnGatheringComplete` was never emitted: 30s measured here on a relay-only peer with
//! one such server, and no deadline outlasts never.
//!
//! The fix is in rtc-turn, which now reports an exhausted `AllocateAttempt` under the id
//! `allocate()` returned (webrtc-rs/rtc#281). This test is the webrtc-level statement of
//! what that buys: the whole peer connection gives up on a silent server on schedule.
use std::net::UdpSocket;
use std::sync::Arc;
use std::time::{Duration, Instant};

use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceGatheringState, RTCIceServer, RTCIceTransportPolicy,
};
use webrtc::runtime::{Sender, channel};

mod common;
use common::{block_on, runtime, timeout};

/// The unreliable schedule ends at 7.8s. Twenty seconds is room for a loaded CI box and
/// still nowhere near the "never" this is a regression test for.
const GATHER_LIMIT: Duration = Duration::from_secs(20);

struct Handler {
    gather_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }
}

/// A bound UDP socket that reads every request and answers none of them.
fn silent_turn_server() -> std::net::SocketAddr {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("a socket");
    let addr = socket.local_addr().expect("its address");
    std::thread::spawn(move || {
        let mut buf = [0u8; 1500];
        while socket.recv_from(&mut buf).is_ok() {}
    });
    addr
}

#[test]
fn a_silent_udp_turn_server_does_not_hold_gathering_for_ever() {
    block_on(async {
        let server = silent_turn_server();
        let (gather_tx, mut gather_rx) = channel::<()>(1);

        let peer = PeerConnectionBuilder::new()
            .with_configuration(
                RTCConfigurationBuilder::new()
                    .with_ice_servers(vec![RTCIceServer {
                        urls: vec![format!("turn:{server}")],
                        username: "user".to_owned(),
                        credential: "pass".to_owned(),
                    }])
                    .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
                    .build(),
            )
            .with_handler(Arc::new(Handler { gather_tx }))
            .with_runtime(runtime())
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .build()
            .await
            .expect("a peer connection");

        let _dc = peer
            .create_data_channel("silent", None)
            .await
            .expect("a data channel");
        let offer = peer.create_offer(None).await.expect("an offer");
        let started = Instant::now();
        peer.set_local_description(offer)
            .await
            .expect("set_local_description");

        timeout(GATHER_LIMIT, gather_rx.recv())
            .await
            .expect("gathering must finish without an answer from the server");
        eprintln!("gathering finished in {:?}", started.elapsed());

        let sdp = peer.local_description().await.expect("a description");
        assert!(
            !sdp.sdp.contains("typ relay"),
            "this server granted no allocation, so there is no relay to offer:\n{}",
            sdp.sdp
        );

        peer.close().await.expect("close");
    });
}
