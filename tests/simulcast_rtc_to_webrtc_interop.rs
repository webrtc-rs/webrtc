/// Integration test for TRUE simulcast with RID (rtc → webrtc)
///
/// This test demonstrates TRUE simulcast by having rtc send 3 simulcast layers
/// with proper RID header extensions, and webrtc receive them.
///
/// **Test flow:**
/// 1. rtc creates 3 tracks with RIDs ("low"/"mid"/"high") and adds to peer
/// 2. rtc creates offer with proper simulcast SDP
/// 3. webrtc (new async API) receives offer and creates answer
/// 4. rtc sends RTP packets with RID header extensions on each track
/// 5. webrtc receives packets via TrackRemoteEvent::OnRtpPacket per RID
/// 6. Test verifies webrtc received packets from all 3 simulcast layers
use anyhow::Result;
use bytes::BytesMut;
use futures::FutureExt;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::RTCPeerConnectionBuilder;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::media_engine::{
    MIME_TYPE_OPUS, MIME_TYPE_VP8, MediaEngine,
};
use rtc::peer_connection::configuration::setting_engine::SettingEngine;
use rtc::peer_connection::event::RTCPeerConnectionEvent;
use rtc::peer_connection::state::{RTCIceConnectionState, RTCPeerConnectionState};
use rtc::peer_connection::transport::RTCDtlsRole;
use rtc::peer_connection::transport::{CandidateConfig, CandidateHostConfig, RTCIceCandidate};
use rtc::rtp;
use rtc::rtp_transceiver::RTCRtpTransceiverDirection;
use rtc::rtp_transceiver::RTCRtpTransceiverInit;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RtpCodecKind};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RTCRtpHeaderExtensionCapability,
};
use rtc::sansio::Protocol;
use rtc::shared::error::Error;
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use webrtc::media_stream::track_remote::TrackRemoteEvent;
use webrtc::peer_connection::RTCIceGatheringState;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Sender, block_on, channel, default_runtime, sleep};

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(30);

struct WebrtcHandler {
    gather_complete_tx: Sender<()>,
    state_tx: Sender<RTCPeerConnectionState>,
    /// Counts RTP packets received per RID
    packets_received_tx: Sender<String>,
    runtime: Arc<dyn webrtc::runtime::Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for WebrtcHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        let _ = self.state_tx.try_send(state);
    }

    async fn on_track(&self, track: Arc<dyn webrtc::media_stream::track_remote::TrackRemote>) {
        let tx = self.packets_received_tx.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(evt) = track.poll().await {
                match evt {
                    TrackRemoteEvent::OnOpen(init) => {
                        let rid = init.rid.as_deref().unwrap_or("").to_string();
                        log::info!("WebRTC track opened: rid={rid} ssrc={}", init.ssrc);
                    }
                    TrackRemoteEvent::OnRtpPacket(pkt) => {
                        let ssrc = pkt.header.ssrc;
                        let _ = tx.try_send(format!("ssrc:{ssrc}"));
                    }
                    TrackRemoteEvent::OnEnded => break,
                    _ => {}
                }
            }
        }));
    }
}

/// Test simulcast: rtc sends 3 layers with RIDs -> webrtc receives all 3 layers
#[test]
fn test_simulcast_rtc_to_webrtc() {
    block_on(run_test()).unwrap();
}

async fn run_test() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    log::info!("Starting simulcast test: rtc (sender) -> webrtc (receiver)");

    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>(1);
    let (state_tx, mut state_rx) = channel::<RTCPeerConnectionState>(16);
    let (packets_tx, mut packets_rx) = channel::<String>(256);

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let config = RTCConfigurationBuilder::new().build();

    // Configure media engine for webrtc side with VP8 codec + simulcast extensions
    let mut webrtc_media_engine = MediaEngine::default();
    let video_codec_for_webrtc = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_VP8.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 96,
        ..Default::default()
    };
    webrtc_media_engine.register_codec(video_codec_for_webrtc, RtpCodecKind::Video)?;
    for extension in [
        "urn:ietf:params:rtp-hdrext:sdes:mid",
        "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id",
        "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id",
    ] {
        webrtc_media_engine.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: extension.to_owned(),
            },
            RtpCodecKind::Video,
            None,
        )?;
    }

    let handler = Arc::new(WebrtcHandler {
        gather_complete_tx,
        state_tx,
        packets_received_tx: packets_tx,
        runtime: runtime.clone(),
    });

    let webrtc_pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_configuration(config.clone())
            .with_media_engine(webrtc_media_engine)
            .with_handler(handler)
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .build()
            .await?,
    );
    log::info!("Created webrtc peer connection");

    // Create rtc peer (offerer / sender)
    let std_socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let rtc_local_addr = std_socket.local_addr()?;
    let rtc_socket = runtime.wrap_udp_socket(std_socket)?;
    log::info!("RTC peer bound to {}", rtc_local_addr);

    let mut setting_engine = SettingEngine::default();
    setting_engine.set_answering_dtls_role(RTCDtlsRole::Server)?;

    let mut media_engine = MediaEngine::default();

    let video_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_VP8.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 96,
        ..Default::default()
    };

    let audio_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            clock_rate: 48000,
            channels: 2,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 120,
        ..Default::default()
    };

    media_engine.register_codec(video_codec.clone(), RtpCodecKind::Video)?;
    media_engine.register_codec(audio_codec.clone(), RtpCodecKind::Audio)?;

    for extension in [
        "urn:ietf:params:rtp-hdrext:sdes:mid",
        "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id",
        "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id",
    ] {
        media_engine.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: extension.to_owned(),
            },
            RtpCodecKind::Video,
            None,
        )?;
    }

    let registry = rtc::interceptor::Registry::new();
    let registry =
        rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors(
            registry,
            &mut media_engine,
        )?;

    let mut rtc_pc = RTCPeerConnectionBuilder::new()
        .with_configuration(config)
        .with_setting_engine(setting_engine)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build()?;
    log::info!("Created RTC peer connection");

    let mid = "0".to_owned();
    let mut rid2ssrc = HashMap::new();
    let mut codings = vec![];
    for rid in ["low", "mid", "high"] {
        let ssrc = rand::random::<u32>();
        rid2ssrc.insert(rid, ssrc);
        codings.push(RTCRtpEncodingParameters {
            rtp_coding_parameters: RTCRtpCodingParameters {
                rid: rid.to_string(),
                ssrc: Some(ssrc),
                ..Default::default()
            },
            codec: video_codec.rtp_codec.clone(),
            ..Default::default()
        });
        log::info!("RTC added track with RID: {} ssrc: {}", rid, ssrc);
    }

    let output_track = MediaStreamTrack::new(
        "webrtc-rs_simulcast".to_string(),
        "video_simulcast".to_string(),
        "video_simulcast".to_string(),
        RtpCodecKind::Video,
        codings,
    );
    let sender_id = rtc_pc.add_track(output_track)?;
    let _ = rtc_pc.add_transceiver_from_kind(
        RtpCodecKind::Audio,
        Some(RTCRtpTransceiverInit {
            direction: RTCRtpTransceiverDirection::Recvonly,
            streams: vec![],
            send_encodings: vec![],
        }),
    );

    let candidate = CandidateHostConfig {
        base_config: CandidateConfig {
            network: "udp".to_owned(),
            address: rtc_local_addr.ip().to_string(),
            port: rtc_local_addr.port(),
            component: 1,
            ..Default::default()
        },
        ..Default::default()
    }
    .new_candidate_host()?;
    rtc_pc.add_local_candidate(RTCIceCandidate::from(&candidate).to_json()?)?;
    log::info!("RTC added local candidate");

    // ──── Signaling ────
    let offer = rtc_pc.create_offer(None)?;
    rtc_pc.set_local_description(offer.clone())?;
    log::info!("RTC created and set offer");

    webrtc_pc
        .set_remote_description(rtc::peer_connection::sdp::RTCSessionDescription::offer(
            offer.sdp.clone(),
        )?)
        .await?;
    log::info!("WebRTC set remote description (offer)");

    let answer = webrtc_pc.create_answer(None).await?;
    webrtc_pc.set_local_description(answer.clone()).await?;
    let _ = webrtc::runtime::timeout(Duration::from_secs(5), gather_complete_rx.recv()).await;
    let answer_with_cands = webrtc_pc
        .local_description()
        .await
        .expect("local description should be set");
    log::info!("WebRTC answer with candidates ready");

    let rtc_answer =
        rtc::peer_connection::sdp::RTCSessionDescription::answer(answer_with_cands.sdp)?;
    rtc_pc.set_remote_description(rtc_answer)?;
    log::info!("RTC set remote description (answer)");

    // ──── Connection + streaming loop ────
    let mut buf = vec![0u8; 2000];
    let mut rtc_connected = false;
    let mut webrtc_connected = false;
    let mut streaming_started = false;
    let mut ssrcs_received: HashMap<u32, u32> = HashMap::new();

    let start_time = Instant::now();
    let test_timeout = Duration::from_secs(15);
    let dummy_frame = vec![0xAA; 500];
    let mut sequence_number: u16 = 0;

    loop {
        if start_time.elapsed() > test_timeout {
            return Err(anyhow::anyhow!("Test timeout"));
        }

        // Flush rtc writes
        while let Some(msg) = rtc_pc.poll_write() {
            if let Err(err) = rtc_socket
                .send_to(&msg.message, msg.transport.peer_addr)
                .await
            {
                log::error!("RTC socket write error: {}", err);
            }
        }

        // Process rtc events
        while let Some(event) = rtc_pc.poll_event() {
            match event {
                RTCPeerConnectionEvent::OnIceConnectionStateChangeEvent(state) => {
                    log::info!("RTC ICE state: {}", state);
                    if state == RTCIceConnectionState::Failed {
                        return Err(anyhow::anyhow!("RTC ICE connection failed"));
                    }
                }
                RTCPeerConnectionEvent::OnConnectionStateChangeEvent(state) => {
                    log::info!("RTC peer connection state: {}", state);
                    if state == RTCPeerConnectionState::Failed {
                        return Err(anyhow::anyhow!("RTC peer connection failed"));
                    }
                    if state == RTCPeerConnectionState::Connected {
                        rtc_connected = true;
                        log::info!("RTC connected!");
                    }
                }
                _ => {}
            }
        }

        // Drain webrtc state changes
        while let Ok(state) = state_rx.try_recv() {
            if state == RTCPeerConnectionState::Connected {
                webrtc_connected = true;
                log::info!("WebRTC connected!");
            }
        }

        // Drain received RTP notifications from webrtc
        while let Ok(tag) = packets_rx.try_recv() {
            if let Some(ssrc_str) = tag.strip_prefix("ssrc:") {
                if let Ok(ssrc) = ssrc_str.parse::<u32>() {
                    *ssrcs_received.entry(ssrc).or_insert(0) += 1;
                }
            }
        }

        if rtc_connected && webrtc_connected && !streaming_started {
            log::info!("Both peers connected, starting simulcast streaming...");
            streaming_started = true;
        }

        if streaming_started {
            let mut rtp_sender = rtc_pc
                .rtp_sender(sender_id)
                .ok_or(Error::ErrRTPSenderNotExisted)?;
            let params = rtp_sender.get_parameters();

            let mut mid_id = None;
            let mut rid_id = None;
            for ext in &params.rtp_parameters.header_extensions {
                if ext.uri == "urn:ietf:params:rtp-hdrext:sdes:mid" {
                    mid_id = Some(ext.id as u8);
                } else if ext.uri == "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id" {
                    rid_id = Some(ext.id as u8);
                }
            }

            for (rid, ssrc) in &rid2ssrc {
                let mut header = rtp::header::Header {
                    version: 2,
                    padding: false,
                    marker: false,
                    payload_type: 96,
                    sequence_number,
                    timestamp: (start_time.elapsed().as_millis() * 90) as u32,
                    ssrc: *ssrc,
                    ..Default::default()
                };
                if let Some(id) = mid_id {
                    header
                        .set_extension(id, bytes::Bytes::from(mid.as_bytes().to_vec()))
                        .ok();
                }
                if let Some(id) = rid_id {
                    header
                        .set_extension(id, bytes::Bytes::from(rid.as_bytes().to_vec()))
                        .ok();
                }
                let packet = rtp::packet::Packet {
                    header,
                    payload: bytes::Bytes::from(dummy_frame.clone()),
                };
                if let Err(e) = rtp_sender.write_rtp(packet) {
                    log::debug!("Failed to send RTP on {}: {}", rid, e);
                }
                sequence_number = sequence_number.wrapping_add(1);
            }

            // Check if we have enough packets across all 3 SSRCs
            let total: u32 = ssrcs_received.values().sum();
            if ssrcs_received.len() >= 3 && total >= 30 {
                log::info!(
                    "Received {} packets across {} SSRCs, test complete",
                    total,
                    ssrcs_received.len()
                );
                break;
            }
        }

        let eto = rtc_pc
            .poll_timeout()
            .unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION);
        let delay = eto
            .checked_duration_since(Instant::now())
            .unwrap_or_default();
        if delay.is_zero() {
            rtc_pc.handle_timeout(Instant::now())?;
            continue;
        }

        let timer = sleep(delay.min(Duration::from_millis(33)));
        futures::select! {
            _ = timer.fuse() => { rtc_pc.handle_timeout(Instant::now())?; }
            res = rtc_socket.recv_from(&mut buf).fuse() => {
                match res {
                    Ok((n, peer_addr)) => {
                        rtc_pc.handle_read(TaggedBytesMut {
                            now: Instant::now(),
                            transport: TransportContext {
                                local_addr: rtc_local_addr,
                                peer_addr,
                                ecn: None,
                                transport_protocol: TransportProtocol::UDP,
                            },
                            message: BytesMut::from(&buf[..n]),
                        })?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }

    // Drain any remaining notifications
    while let Ok(tag) = packets_rx.try_recv() {
        if let Some(ssrc_str) = tag.strip_prefix("ssrc:") {
            if let Ok(ssrc) = ssrc_str.parse::<u32>() {
                *ssrcs_received.entry(ssrc).or_insert(0) += 1;
            }
        }
    }

    log::info!("Final RTP counts by SSRC:");
    for (ssrc, count) in &ssrcs_received {
        log::info!("  ssrc={}: {} packets", ssrc, count);
    }

    assert!(
        ssrcs_received.len() >= 3,
        "Expected packets on 3 SSRCs (one per RID), got {}",
        ssrcs_received.len()
    );
    let total: u32 = ssrcs_received.values().sum();
    assert!(
        total >= 30,
        "Expected at least 30 total packets, got {}",
        total
    );

    log::info!(
        "✅ SUCCESS: {} packets across {} simulcast layers",
        total,
        ssrcs_received.len()
    );

    webrtc_pc.close().await?;
    rtc_pc.close()?;
    Ok(())
}
