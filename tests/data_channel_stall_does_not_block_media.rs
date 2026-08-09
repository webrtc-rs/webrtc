//! A stalled data-channel consumer must not stop media on the same peer connection.
//!
//! The driver applies data-channel back-pressure by refusing to pull from the core — that is
//! what lets the backlog build where the SCTP handler can see it, so `a_rwnd` falls and the
//! peer throttles. But `core.poll_read()` is a **single FIFO carrying RTP and RTCP as well**,
//! so "stop pulling" stops media too.
//!
//! The two have nothing to do with each other. An SFU forwarding video while an application
//! task is briefly slow to drain a signalling channel should not lose the video. SCTP's
//! receive window is per-association and carries only data channels; RTP arrives over SRTP
//! through a different demux path entirely, and is subject to none of it.
//!
//! This test stalls a data-channel consumer hard enough to engage retention, then checks that
//! RTP keeps being delivered while that stall is in progress.

use anyhow::Result;
use bytes::BytesMut;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_VP8, MediaEngine};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};

use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, channel};

mod common;
use common::{block_on, runtime, sleep, timeout};

const VIDEO_SSRC: u32 = 0x00D0_1234;

/// Comfortably past `DRIVER_TO_DATA_CHANNEL_EVENT_CHANNEL_CAPACITY` (256), so the driver is
/// genuinely retaining events rather than fitting everything in the hand-off channel.
const DATA_MESSAGES: u32 = 1_500;

/// How long media is observed while the data-channel consumer is stalled.
const OBSERVE_WINDOW: Duration = Duration::from_secs(2);

struct SenderHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for SenderHandler {
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

/// Drains media promptly; never drains the data channel.
struct MixedReceiverHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
    rtp_seen: Arc<AtomicU32>,
    dc_seen: Arc<AtomicBool>,
    /// The `DataChannel` must be *held*, not dropped. Dropping it closes the receiving end,
    /// and the driver would then hit `Disconnected` — a different path that retains nothing,
    /// which would make this test pass for the wrong reason.
    held_dc: std::sync::Mutex<Option<Arc<dyn DataChannel>>>,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for MixedReceiverHandler {
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
    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        *self.held_dc.lock().unwrap() = Some(dc);
        self.dc_seen.store(true, Ordering::Release);
        // and then nothing: this consumer never calls `poll()`.
    }
    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let rtp_seen = self.rtp_seen.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(evt) = track.poll().await {
                match evt {
                    TrackRemoteEvent::OnRtpPacket(_) => {
                        rtp_seen.fetch_add(1, Ordering::Relaxed);
                    }
                    TrackRemoteEvent::OnEnded => break,
                    _ => {}
                }
            }
        }));
    }
}

fn vp8_media_engine() -> MediaEngine {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_codec(
            RTCRtpCodecParameters {
                rtp_codec: RTCRtpCodec {
                    mime_type: MIME_TYPE_VP8.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 96,
                ..Default::default()
            },
            RtpCodecKind::Video,
        )
        .expect("register VP8");
    media_engine
}

fn video_track() -> Arc<TrackLocalStaticRTP> {
    Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
        "mixed-stream".to_owned(),
        "mixed-video".to_owned(),
        "mixed-label".to_owned(),
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

async fn media_survives_a_data_channel_stall() -> Result<()> {
    let runtime = runtime();

    let (snd_gather_tx, mut snd_gather_rx) = channel::<()>(1);
    let (snd_conn_tx, mut snd_conn_rx) = channel::<()>(1);
    let (rcv_gather_tx, mut rcv_gather_rx) = channel::<()>(1);
    let (rcv_conn_tx, mut rcv_conn_rx) = channel::<()>(1);

    let sender_pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_handler(Arc::new(SenderHandler {
                gather_tx: snd_gather_tx,
                connected_tx: snd_conn_tx,
            }))
            .with_media_engine(vp8_media_engine())
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .build()
            .await?,
    );

    let track = video_track();
    sender_pc
        .add_track(Arc::clone(&track) as Arc<dyn TrackLocal>)
        .await?;

    let dc = sender_pc
        .create_data_channel("chatty", Some(RTCDataChannelInit::default()))
        .await?;
    let (open_tx, mut open_rx) = channel::<()>(1);
    {
        let dc = dc.clone();
        runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        let _ = open_tx.try_send(());
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }

    let rtp_seen = Arc::new(AtomicU32::new(0));
    let dc_seen = Arc::new(AtomicBool::new(false));
    let receiver_pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_handler(Arc::new(MixedReceiverHandler {
                gather_tx: rcv_gather_tx,
                connected_tx: rcv_conn_tx,
                rtp_seen: rtp_seen.clone(),
                dc_seen: dc_seen.clone(),
                held_dc: std::sync::Mutex::new(None),
                runtime: runtime.clone(),
            }))
            .with_media_engine(vp8_media_engine())
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .build()
            .await?,
    );

    let offer = sender_pc.create_offer(None).await?;
    sender_pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), snd_gather_rx.recv()).await;
    let offer_sdp = sender_pc.local_description().await.expect("offer");

    receiver_pc.set_remote_description(offer_sdp).await?;
    let answer = receiver_pc.create_answer(None).await?;
    receiver_pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), rcv_gather_rx.recv()).await;
    let answer_sdp = receiver_pc.local_description().await.expect("answer");
    sender_pc.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), snd_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: sender connect"))?;
    timeout(Duration::from_secs(15), rcv_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: receiver connect"))?;
    timeout(Duration::from_secs(10), open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: data channel did not open"))?;

    // Media flows continuously for the whole test, paced like a real encoder.
    {
        let track = Arc::clone(&track);
        runtime.spawn(Box::pin(async move {
            for seq in 0u16..u16::MAX {
                let packet = rtc::rtp::packet::Packet {
                    header: rtc::rtp::header::Header {
                        version: 2,
                        payload_type: 96,
                        sequence_number: seq,
                        timestamp: (seq as u32).wrapping_mul(3000),
                        ssrc: VIDEO_SSRC,
                        ..Default::default()
                    },
                    payload: bytes::Bytes::from(vec![0xAAu8; 200]),
                };
                if track.write_rtp(packet).await.is_err() {
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
        }));
    }

    // Stall the data channel: push far more than the hand-off channel holds at a consumer
    // that never reads, so the driver is forced to retain.
    for i in 0..DATA_MESSAGES {
        let mut buf = BytesMut::from(&i.to_be_bytes()[..]);
        buf.resize(64, 0);
        dc.send(buf).await?;
    }

    for _ in 0..100 {
        if dc_seen.load(Ordering::Acquire) {
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }
    assert!(
        dc_seen.load(Ordering::Acquire),
        "receiver never saw the data channel, so it was never stalled"
    );

    // Let the retention take hold, then watch media across a window.
    sleep(Duration::from_millis(500)).await;
    let before = rtp_seen.load(Ordering::Relaxed);
    sleep(OBSERVE_WINDOW).await;
    let after = rtp_seen.load(Ordering::Relaxed);

    assert!(
        before > 0,
        "no media arrived at all before the observation window — the test never got \
         into the state it is checking"
    );
    assert!(
        after > before,
        "media stopped while a data-channel consumer was stalled: {before} packets before \
         the window, {after} after {OBSERVE_WINDOW:?}. The driver stops pulling from the \
         core to apply data-channel back-pressure, and that same FIFO carries RTP — so a \
         slow data-channel consumer is freezing video it has nothing to do with."
    );

    sender_pc.close().await?;
    receiver_pc.close().await?;
    Ok(())
}

#[test]
fn test_data_channel_stall_does_not_block_media() {
    block_on(media_survives_a_data_channel_stall()).unwrap();
}
