//! Worked example: reading the peers' DTLS certificate fingerprints out of `get_stats`.
//!
//! 0.17 had `RTCDtlsTransport::get_remote_certificate`; on 0.20 the statistics report is the
//! route. Certificate-pinning protocols need the *remote* fingerprint — libp2p's WebRTC-Direct,
//! for one, binds both peers' fingerprints into its Noise handshake prologue, and a listener
//! can only learn the dialer's from the certificate the peer actually presented.
//!
//! The lookup is a two-step because the report identifies certificates indirectly: it carries a
//! `Certificate` entry per side and neither says which side it belongs to — only the `Transport`
//! entry's `local_certificate_id` / `remote_certificate_id` tell them apart. A cross-check pins
//! that down: what one peer sees as *remote* must equal what the other reports as its *local*
//! certificate, in both directions.
//!
//! Getting this backwards is not a loud failure. A pinning protocol would compare a fingerprint
//! against itself and happily accept every peer.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCStatsReportEntry,
    StatsSelector,
};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Sender, channel};

mod common;
use common::{block_on, runtime, timeout};

/// Which end of the connection a certificate belongs to.
#[derive(Clone, Copy)]
enum Side {
    Local,
    Remote,
}

/// The SHA-256 fingerprint of one side's DTLS certificate, colon-separated hex — the same shape
/// as an SDP `a=fingerprint:` value ([RFC 4572 §5]). `None` before the handshake has completed.
///
/// [RFC 4572 §5]: https://www.rfc-editor.org/rfc/rfc4572#section-5
async fn certificate_fingerprint(
    pc: &dyn PeerConnection,
    now: Instant,
    side: Side,
) -> Option<String> {
    let report = pc.get_stats(now, StatsSelector::None).await;

    let id = report.iter().find_map(|entry| match entry {
        RTCStatsReportEntry::Transport(transport) => {
            let id = match side {
                Side::Local => &transport.local_certificate_id,
                Side::Remote => &transport.remote_certificate_id,
            };
            (!id.is_empty()).then(|| id.clone())
        }
        _ => None,
    })?;

    report.iter().find_map(|entry| match entry {
        RTCStatsReportEntry::Certificate(cert) if cert.stats.id == id => {
            Some(cert.fingerprint.clone())
        }
        _ => None,
    })
}

struct Handler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }
}

struct Peer {
    pc: Box<dyn PeerConnection>,
    gather_rx: webrtc::runtime::Receiver<()>,
    connected_rx: webrtc::runtime::Receiver<()>,
}

impl Peer {
    async fn fingerprint(&self, now: Instant, side: Side) -> Option<String> {
        certificate_fingerprint(&*self.pc, now, side).await
    }
}

async fn build_peer(runtime: Arc<dyn webrtc::runtime::Runtime>) -> Result<Peer> {
    let (gather_tx, gather_rx) = channel::<()>(1);
    let (connected_tx, connected_rx) = channel::<()>(1);

    let pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx,
            connected_tx,
        }))
        .with_runtime(runtime)
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    Ok(Peer {
        pc: Box::new(pc),
        gather_rx,
        connected_rx,
    })
}

#[test]
fn test_remote_certificate_fingerprint_is_the_peers() {
    block_on(run()).unwrap();
}

async fn run() -> Result<()> {
    let runtime = runtime();

    let mut offerer = build_peer(runtime.clone()).await?;
    let mut answerer = build_peer(runtime.clone()).await?;

    // Nothing has been presented yet.
    assert_eq!(
        offerer.fingerprint(Instant::now(), Side::Remote).await,
        None,
        "there is no remote certificate before the handshake"
    );

    // A data channel gives the connection something to negotiate.
    offerer.pc.create_data_channel("probe", None).await?;

    let offer = offerer.pc.create_offer(None).await?;
    offerer.pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), offerer.gather_rx.recv()).await;
    let offer_sdp = offerer
        .pc
        .local_description()
        .await
        .expect("offerer local description");

    answerer.pc.set_remote_description(offer_sdp).await?;
    let answer = answerer.pc.create_answer(None).await?;
    answerer.pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), answerer.gather_rx.recv()).await;
    let answer_sdp = answerer
        .pc
        .local_description()
        .await
        .expect("answerer local description");
    offerer.pc.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), offerer.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: offerer connect"))?;
    timeout(Duration::from_secs(5), answerer.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: answerer connect"))?;

    let now = Instant::now();
    let offerer_sees = offerer
        .fingerprint(now, Side::Remote)
        .await
        .expect("offerer should see the answerer's certificate");
    let answerer_sees = answerer
        .fingerprint(now, Side::Remote)
        .await
        .expect("answerer should see the offerer's certificate");

    let offerer_own = offerer
        .fingerprint(now, Side::Local)
        .await
        .expect("offerer local certificate");
    let answerer_own = answerer
        .fingerprint(now, Side::Local)
        .await
        .expect("answerer local certificate");

    // The cross-check: each side's "remote" is the other side's "local".
    assert_eq!(
        offerer_sees, answerer_own,
        "what the offerer sees as remote must be the answerer's certificate"
    );
    assert_eq!(
        answerer_sees, offerer_own,
        "what the answerer sees as remote must be the offerer's certificate"
    );

    // Guards the cross-check above: were both peers to reuse one certificate, the two
    // assertions would hold no matter which side the lookup returned.
    assert_ne!(
        offerer_own, answerer_own,
        "the two peers must have distinct certificates for this test to mean anything"
    );

    // Shape check -- colon-separated SHA-256 hex, per RFC 4572 section 5.
    assert_eq!(
        offerer_sees.split(':').count(),
        32,
        "expected 32 colon-separated bytes, got {offerer_sees:?}"
    );

    Ok(())
}
