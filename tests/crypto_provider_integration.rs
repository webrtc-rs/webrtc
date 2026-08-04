//! End-to-end validation of crypto-provider selection through the public async API.
//!
//! Every combination below is driven entirely through `PeerConnectionBuilder` and
//! `SettingEngine::set_crypto_provider` — no reaching into `rtc` internals — and each run
//! exercises the whole crypto surface of a connection:
//!
//! * **ICE / STUN** — connectivity checks are authenticated with MESSAGE-INTEGRITY (HMAC-SHA1),
//!   so reaching `Connected` at all means STUN integrity worked on both sides.
//! * **DTLS** — the handshake runs the PRF, key exchange, signatures, and certificate handling.
//! * **SRTP** — a VP8 track is written as RTP and read on the far side, so media is encrypted
//!   with one provider and decrypted by the other.
//! * **Certificates** — SHA-256 fingerprints are computed through the provider and compared
//!   across the two peers.
//!
//! The cross-provider cases are the point: `ring` on one endpoint and `aws-lc-rs` on the other
//! must interoperate, because a provider is a local implementation choice and must never change
//! what goes on the wire.

#![cfg(any(feature = "crypto-ring", feature = "crypto-aws-lc-rs"))]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use anyhow::Result;

use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_VP8, MediaEngine};
use rtc::peer_connection::configuration::setting_engine::SettingEngine;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::crypto;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, channel};

mod common;
use common::{block_on, interval, runtime, sleep, timeout};

const VIDEO_SSRC: u32 = 0x5EED_1234;
const VIDEO_PAYLOAD_TYPE: u8 = 96;

// ── Provider selection ─────────────────────────────────────────────────────────

/// Which provider an endpoint should use, named so failures say which pairing broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Provider {
    #[cfg(feature = "crypto-ring")]
    Ring,
    #[cfg(feature = "crypto-aws-lc-rs")]
    AwsLcRs,
    /// An application-supplied provider: a thin wrapper that delegates to a built-in and counts
    /// calls, standing in for a downstream HSM or FIPS implementation.
    Application,
}

impl Provider {
    fn build(self, calls: &Arc<AtomicU32>) -> Arc<dyn crypto::RTCCryptoProvider> {
        match self {
            #[cfg(feature = "crypto-ring")]
            Provider::Ring => Arc::new(crypto::providers::RingProvider::new()),
            #[cfg(feature = "crypto-aws-lc-rs")]
            Provider::AwsLcRs => Arc::new(crypto::providers::AwsLcRsProvider::new()),
            Provider::Application => Arc::new(CountingProvider {
                inner: crypto::default_provider().expect("a built-in provider is enabled"),
                calls: Arc::clone(calls),
            }),
        }
    }
}

/// An application provider. It delegates rather than reimplementing, because the point is to
/// prove the *plumbing* accepts an arbitrary implementation — `rtc-crypto`'s conformance suite is
/// what validates a real one's correctness.
struct CountingProvider {
    inner: Arc<dyn crypto::RTCCryptoProvider>,
    calls: Arc<AtomicU32>,
}

impl std::fmt::Debug for CountingProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `RTCCryptoProvider` has no `Debug` — provider state can hold key material — so report
        // the non-secret name only.
        formatter
            .debug_struct("CountingProvider")
            .field("inner", &self.inner.name())
            .finish()
    }
}

impl crypto::RTCCryptoProvider for CountingProvider {
    fn name(&self) -> &'static str {
        "application-test-provider"
    }

    fn crypto(&self) -> &dyn crypto::RTCCrypto {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.crypto()
    }

    fn random(&self) -> &dyn crypto::RTCRandom {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.random()
    }
}

// ── Handlers ───────────────────────────────────────────────────────────────────

struct OffererHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for OffererHandler {
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

struct AnswererHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
    rtp_count: Arc<AtomicU32>,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for AnswererHandler {
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

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        // Every packet counted here was SRTP-decrypted by this endpoint's provider after being
        // SRTP-encrypted by the other's.
        let rtp_count = Arc::clone(&self.rtp_count);
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = track.poll().await {
                match event {
                    TrackRemoteEvent::OnRtpPacket(_) => {
                        rtp_count.fetch_add(1, Ordering::Relaxed);
                    }
                    TrackRemoteEvent::OnEnded => break,
                    _ => {}
                }
            }
        }));
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn vp8_media_engine() -> Result<MediaEngine> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_codec(
        RTCRtpCodecParameters {
            rtp_codec: RTCRtpCodec {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: VIDEO_PAYLOAD_TYPE,
        },
        RtpCodecKind::Video,
    )?;
    Ok(media_engine)
}

fn setting_engine_with(provider: Arc<dyn crypto::RTCCryptoProvider>) -> SettingEngine {
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_crypto_provider(provider);
    setting_engine
}

fn video_track() -> Arc<TrackLocalStaticRTP> {
    Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
        "provider-test-stream".to_owned(),
        "provider-test-video".to_owned(),
        "provider-test-label".to_owned(),
        RtpCodecKind::Video,
        vec![RTCRtpEncodingParameters {
            rtp_coding_parameters: RTCRtpCodingParameters {
                ssrc: Some(VIDEO_SSRC),
                ..Default::default()
            },
            codec: RTCRtpCodec {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            ..Default::default()
        }],
    )))
}

/// Connects two peers, each with its own provider, and drives media across the link.
async fn connect_and_exchange(offerer: Provider, answerer: Provider) -> Result<()> {
    let runtime = runtime();
    let app_calls = Arc::new(AtomicU32::new(0));
    let uses_application = offerer == Provider::Application || answerer == Provider::Application;

    let (off_gather_tx, mut off_gather_rx) = channel::<()>(1);
    let (off_conn_tx, mut off_conn_rx) = channel::<()>(1);
    let (ans_gather_tx, mut ans_gather_rx) = channel::<()>(1);
    let (ans_conn_tx, mut ans_conn_rx) = channel::<()>(1);
    let rtp_count = Arc::new(AtomicU32::new(0));

    let offerer_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(OffererHandler {
            gather_tx: off_gather_tx,
            connected_tx: off_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_setting_engine(setting_engine_with(offerer.build(&app_calls)))
        .with_media_engine(vp8_media_engine()?)
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let track = video_track();
    offerer_pc
        .add_track(Arc::clone(&track) as Arc<dyn TrackLocal>)
        .await?;

    let answerer_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(AnswererHandler {
            gather_tx: ans_gather_tx,
            connected_tx: ans_conn_tx,
            rtp_count: Arc::clone(&rtp_count),
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_setting_engine(setting_engine_with(answerer.build(&app_calls)))
        .with_media_engine(vp8_media_engine()?)
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    // ── Offer / answer ────────────────────────────────────────────────────────
    let offer = offerer_pc.create_offer(None).await?;
    offerer_pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), off_gather_rx.recv()).await;
    let offer_sdp = offerer_pc
        .local_description()
        .await
        .expect("offerer local description");

    answerer_pc.set_remote_description(offer_sdp).await?;
    let answer = answerer_pc.create_answer(None).await?;
    answerer_pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), ans_gather_rx.recv()).await;
    let answer_sdp = answerer_pc
        .local_description()
        .await
        .expect("answerer local description");
    offerer_pc.set_remote_description(answer_sdp).await?;

    // Reaching Connected means ICE (STUN MESSAGE-INTEGRITY) and the DTLS handshake both
    // succeeded under this provider pairing.
    timeout(Duration::from_secs(20), off_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("offerer never connected ({offerer:?} -> {answerer:?})"))?;
    timeout(Duration::from_secs(10), ans_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("answerer never connected ({offerer:?} -> {answerer:?})"))?;

    // ── Media across the link (SRTP) ──────────────────────────────────────────
    let mut ticker = interval(Duration::from_millis(5));
    for sequence_number in 0u16..60 {
        let packet = rtc::rtp::packet::Packet {
            header: rtc::rtp::header::Header {
                version: 2,
                payload_type: VIDEO_PAYLOAD_TYPE,
                sequence_number,
                timestamp: u32::from(sequence_number).wrapping_mul(3000),
                ssrc: VIDEO_SSRC,
                ..Default::default()
            },
            payload: bytes::Bytes::from(vec![0xABu8; 120]),
        };
        let _ = track.write_rtp(packet).await;
        let _ = ticker.tick().await;
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while rtp_count.load(Ordering::Relaxed) == 0 && std::time::Instant::now() < deadline {
        sleep(Duration::from_millis(50)).await;
    }
    assert!(
        rtp_count.load(Ordering::Relaxed) > 0,
        "no SRTP media arrived ({offerer:?} -> {answerer:?}); \
         the connection came up but media did not decrypt"
    );

    if uses_application {
        assert!(
            app_calls.load(Ordering::Relaxed) > 0,
            "the application-supplied provider was configured but never used"
        );
    }

    sleep(Duration::from_millis(100)).await;
    offerer_pc.close().await?;
    answerer_pc.close().await?;
    Ok(())
}

// ── Same-provider endpoints ────────────────────────────────────────────────────

#[cfg(feature = "crypto-ring")]
#[test]
fn ring_to_ring() -> Result<()> {
    block_on(connect_and_exchange(Provider::Ring, Provider::Ring))
}

#[cfg(feature = "crypto-aws-lc-rs")]
#[test]
fn aws_lc_rs_to_aws_lc_rs() -> Result<()> {
    block_on(connect_and_exchange(Provider::AwsLcRs, Provider::AwsLcRs))
}

/// An application provider on both ends, proving `SettingEngine` accepts an arbitrary
/// implementation with no built-in involvement in the plumbing.
#[test]
fn application_provider_to_application_provider() -> Result<()> {
    block_on(connect_and_exchange(
        Provider::Application,
        Provider::Application,
    ))
}

// ── Cross-provider endpoints ───────────────────────────────────────────────────
//
// The interoperability claim: a provider is a local implementation detail, so two endpoints on
// different backends must produce a working connection. Only compiled when both are available.

#[cfg(all(feature = "crypto-ring", feature = "crypto-aws-lc-rs"))]
#[test]
fn ring_offerer_to_aws_lc_rs_answerer() -> Result<()> {
    block_on(connect_and_exchange(Provider::Ring, Provider::AwsLcRs))
}

#[cfg(all(feature = "crypto-ring", feature = "crypto-aws-lc-rs"))]
#[test]
fn aws_lc_rs_offerer_to_ring_answerer() -> Result<()> {
    block_on(connect_and_exchange(Provider::AwsLcRs, Provider::Ring))
}

#[cfg(feature = "crypto-ring")]
#[test]
fn application_provider_to_ring() -> Result<()> {
    block_on(connect_and_exchange(Provider::Application, Provider::Ring))
}
