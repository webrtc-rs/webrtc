//! Reproduction for issue #835 — ICE restart after TURN credential rotation must not 437.
//!
//! Rotating TURN credentials and applying them with `set_configuration` + `restart_ice()`
//! made the relayer tear down its clients and re-`Allocate` on the **same 5-tuple**, because
//! `local_addrs` is fixed at relayer construction. The server still holds the prior
//! allocation for that 5-tuple, so RFC 5766 §6.2 requires it to answer **437 (Allocation
//! Mismatch)** — the restart then gathers no relay candidate and the data path drops.
//!
//! This test needs a TURN server that actually enforces §6.2. The mock in `ice_test.rs`
//! does not (it answers every Allocate with success), which is precisely why the #832 test
//! passes while this bug remains. So it drives a real one:
//!
//! ```shell
//! go build -o /tmp/turn-server ~/pion/turn/examples/turn-server/simple
//! /tmp/turn-server -public-ip 127.0.0.1 -port 3478 -users "user1=pass1,user2=pass2" -realm webrtc.rs &
//! WEBRTC_TURN_SERVER=127.0.0.1:3478 cargo test --test turn_credential_rotation -- --nocapture
//! ```
//!
//! Without `WEBRTC_TURN_SERVER` the test skips, so it never breaks a CI run that has no
//! TURN server. The assertion is the user-visible symptom: **the restarted gathering still
//! produces a relay candidate**. Before the fix it produces none, and the server logs
//! "relay already allocated for 5-TUPLE".

use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rtc::stun::error_code::ErrorCodeAttribute;
use rtc::stun::message::{
    CLASS_ERROR_RESPONSE, CLASS_REQUEST, Getter, METHOD_ALLOCATE, METHOD_REFRESH,
    Message as StunMessage,
};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCIceCandidateType, RTCIceGatheringState, RTCIceServer,
    RTCIceTransportPolicy, RTCPeerConnectionIceEvent,
};
use webrtc::runtime::{Mutex, Sender, channel};

mod common;
use common::{block_on, timeout};

/// What the proxy saw on the wire, which is the only signal that distinguishes the bug from
/// the fix: the SDP keeps the stale relay candidate either way, and an unchanged candidate is
/// (correctly) not re-announced through `on_ice_candidate`.
#[derive(Default)]
struct WireCounts {
    allocates: AtomicUsize,
    refreshes: AtomicUsize,
    alloc_mismatch_437: AtomicUsize,
}

/// A UDP proxy that forwards between the client and the real TURN server, counting the TURN
/// methods it sees. Runs on its own thread with blocking sockets — it is test scaffolding,
/// so it has no reason to participate in the async runtime.
fn spawn_stun_counting_proxy(server_addr: SocketAddr) -> (SocketAddr, Arc<WireCounts>) {
    let facing_client = UdpSocket::bind("127.0.0.1:0").expect("bind proxy socket");
    let proxy_addr = facing_client.local_addr().expect("proxy addr");
    facing_client
        .set_read_timeout(Some(Duration::from_millis(50)))
        .expect("proxy read timeout");

    let facing_server = UdpSocket::bind("127.0.0.1:0").expect("bind proxy upstream socket");
    facing_server
        .set_read_timeout(Some(Duration::from_millis(50)))
        .expect("upstream read timeout");

    let counts = Arc::new(WireCounts::default());
    let seen = counts.clone();

    std::thread::spawn(move || {
        let mut client_addr: Option<SocketAddr> = None;
        let mut buf = [0u8; 2048];

        loop {
            // client -> server
            if let Ok((n, from)) = facing_client.recv_from(&mut buf) {
                client_addr = Some(from);
                count_request(&seen, &buf[..n]);
                let _ = facing_server.send_to(&buf[..n], server_addr);
            }

            // server -> client
            if let Ok((n, _)) = facing_server.recv_from(&mut buf)
                && let Some(dst) = client_addr
            {
                count_response(&seen, &buf[..n]);
                let _ = facing_client.send_to(&buf[..n], dst);
            }
        }
    });

    (proxy_addr, counts)
}

fn parse_stun(datagram: &[u8]) -> Option<StunMessage> {
    // ChannelData and other non-STUN traffic simply does not decode; skip it.
    let mut msg = StunMessage::new();
    msg.raw = datagram.to_vec();
    msg.decode().ok()?;
    Some(msg)
}

fn count_request(counts: &WireCounts, datagram: &[u8]) {
    let Some(msg) = parse_stun(datagram) else {
        return;
    };
    if msg.typ.class != CLASS_REQUEST {
        return;
    }
    if msg.typ.method == METHOD_ALLOCATE {
        counts.allocates.fetch_add(1, Ordering::SeqCst);
    } else if msg.typ.method == METHOD_REFRESH {
        counts.refreshes.fetch_add(1, Ordering::SeqCst);
    }
}

fn count_response(counts: &WireCounts, datagram: &[u8]) {
    let Some(msg) = parse_stun(datagram) else {
        return;
    };
    if msg.typ.class != CLASS_ERROR_RESPONSE {
        return;
    }
    let mut code = ErrorCodeAttribute::default();
    if code.get_from(&msg).is_ok() && code.code.0 == 437 {
        counts.alloc_mismatch_437.fetch_add(1, Ordering::SeqCst);
    }
}

struct Tracker {
    candidates: Arc<Mutex<Vec<RTCIceCandidateType>>>,
    gathering_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Tracker {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        println!(
            "  candidate: {:?} {}:{}",
            event.candidate.typ, event.candidate.address, event.candidate.port
        );
        self.candidates.lock().await.push(event.candidate.typ);
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gathering_tx.try_send(());
        }
    }
}

#[test]
fn test_ice_restart_after_turn_credential_rotation_keeps_relay() {
    let Ok(turn_addr) = std::env::var("WEBRTC_TURN_SERVER") else {
        eprintln!("skipping: set WEBRTC_TURN_SERVER=host:port (see the module docs)");
        return;
    };

    block_on(async move {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .is_test(false)
            .try_init()
            .ok();

        let server_addr: SocketAddr = turn_addr.parse().expect("WEBRTC_TURN_SERVER=host:port");
        let (proxy_addr, wire) = spawn_stun_counting_proxy(server_addr);
        let turn_url = format!("turn:{proxy_addr}?transport=udp");
        let server = |user: &str, pass: &str| RTCIceServer {
            urls: vec![turn_url.clone()],
            username: user.to_owned(),
            credential: pass.to_owned(),
        };

        let mut media_engine = MediaEngine::default();
        media_engine.register_default_codecs().unwrap();

        let candidates = Arc::new(Mutex::new(Vec::new()));
        let (gathering_tx, mut gathering_rx) = channel(8);

        let pc = PeerConnectionBuilder::new()
            .with_configuration(
                RTCConfigurationBuilder::new()
                    .with_ice_servers(vec![server("user1", "pass1")])
                    .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
                    .build(),
            )
            .with_media_engine(media_engine)
            .with_handler(Arc::new(Tracker {
                candidates: candidates.clone(),
                gathering_tx,
            }))
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        let _ = pc.create_data_channel("channel1", None).await.unwrap();

        // ── 1. Initial allocation on credential A ─────────────────────────────
        println!("── initial gather (user1) ──");
        let offer = pc.create_offer(None).await.unwrap();
        pc.set_local_description(offer).await.unwrap();
        timeout(Duration::from_secs(10), gathering_rx.recv())
            .await
            .expect("timed out waiting for the initial relay gathering");

        let before: Vec<_> = candidates.lock().await.clone();
        let relays_before = before
            .iter()
            .filter(|t| **t == RTCIceCandidateType::Relay)
            .count();
        assert!(
            relays_before > 0,
            "precondition: the initial Allocate must succeed, got {before:?} — is the TURN \
             server running with user1=pass1?"
        );
        println!("   got {relays_before} relay candidate(s)");

        let before_sdp = pc
            .local_description()
            .await
            .expect("local description before the rotation")
            .sdp;

        candidates.lock().await.clear();

        // ── 2. Rotate credentials and restart ICE ─────────────────────────────
        // Same server, same policy, different credential: a pure credential rotation. The
        // allocation is a 5-tuple resource independent of the credential that created it,
        // so nothing here should require giving it up.
        let allocates_before = wire.allocates.load(Ordering::SeqCst);
        assert!(
            allocates_before > 0,
            "precondition: the proxy should have seen the initial Allocate"
        );

        println!("── rotate to user2 + restart_ice ──");
        pc.set_configuration(
            RTCConfigurationBuilder::new()
                .with_ice_servers(vec![server("user2", "pass2")])
                .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
                .build(),
        )
        .await
        .unwrap();
        pc.restart_ice().await.unwrap();

        // Complete the restart the way an application does: a fresh offer carrying the new
        // ufrag. This is what re-publishes the local candidates to the remote peer.
        let offer = pc.create_offer(None).await.unwrap();
        pc.set_local_description(offer).await.unwrap();
        timeout(Duration::from_secs(10), gathering_rx.recv())
            .await
            .expect("timed out waiting for the post-rotation relay gathering");

        // Assert on the description rather than on `on_ice_candidate`. With the allocation
        // preserved the relay candidate is unchanged, so the core correctly does *not*
        // re-announce it as a new candidate — but it must still be offered to the remote,
        // and that is the property the data path depends on.
        let sdp = pc
            .local_description()
            .await
            .expect("local description after the ICE restart")
            .sdp;
        let relay_lines: Vec<&str> = sdp
            .lines()
            .filter(|l| l.contains("typ relay"))
            .map(str::trim)
            .collect();
        println!("   relay candidates in the restarted offer: {relay_lines:?}");

        assert!(
            !relay_lines.is_empty(),
            "issue #835: the offer after rotating TURN credentials carries no relay \
             candidate. The relayer tore down its clients and re-Allocated on the same \
             5-tuple, so the server answered 437 (Allocation Mismatch) per RFC 5766 §6.2. \
             SDP was:\n{sdp}"
        );

        // The allocation must be the *same* one.
        let before_relay = before_sdp
            .lines()
            .find(|l| l.contains("typ relay"))
            .map(str::trim)
            .expect("a relay candidate before the rotation");
        assert!(
            relay_lines
                .iter()
                .any(|l| relay_line_addr(l) == relay_line_addr(before_relay)),
            "the relayed address changed across the rotation ({before_relay:?} -> \
             {relay_lines:?})"
        );

        // ── What actually happened on the wire ────────────────────────────────
        // These are the assertions with teeth. The SDP above keeps the stale relay
        // candidate whether or not the re-Allocate succeeded, so only the wire distinguishes
        // "re-signed the existing allocation" from "tried to re-Allocate and got 437".
        let allocates_after = wire.allocates.load(Ordering::SeqCst) - allocates_before;
        let refreshes = wire.refreshes.load(Ordering::SeqCst);
        let mismatches = wire.alloc_mismatch_437.load(Ordering::SeqCst);
        println!(
            "   wire: {allocates_after} new Allocate(s), {refreshes} Refresh(es), \
             {mismatches} x 437"
        );

        assert_eq!(
            mismatches, 0,
            "issue #835: the server answered {mismatches} x 437 (Allocation Mismatch). The \
             relayer re-Allocated on the same 5-tuple while the server still held the \
             previous allocation (RFC 5766 §6.2)."
        );
        assert_eq!(
            allocates_after, 0,
            "issue #835: {allocates_after} new Allocate(s) after a pure credential rotation. \
             An allocation is a 5-tuple resource independent of the credential that created \
             it, so rotating credentials must re-sign the existing allocation, not replace it."
        );
        assert!(
            refreshes > 0,
            "expected a Refresh carrying the rotated credential; the server only learns the \
             new credential when the allocation is re-signed"
        );
    });
}

/// `candidate:<foundation> <component> udp <pri> <ip> <port> typ relay ...` -> `<ip>:<port>`.
fn relay_line_addr(line: &str) -> String {
    let f: Vec<&str> = line.split_whitespace().collect();
    format!("{}:{}", f[4], f[5])
}
