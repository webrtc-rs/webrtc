use anyhow::Result;
use bytes::BytesMut;
use futures::FutureExt;
use rtc::interceptor::Registry;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::RTCPeerConnectionBuilder;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_VP8, MediaEngine};
use rtc::peer_connection::configuration::setting_engine::SettingEngine;
use rtc::peer_connection::event::RTCPeerConnectionEvent;
use rtc::peer_connection::state::{RTCIceConnectionState, RTCPeerConnectionState};
use rtc::peer_connection::transport::RTCDtlsRole;
use rtc::peer_connection::transport::{CandidateConfig, CandidateHostConfig, RTCIceCandidate};
use rtc::rtp;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};
use rtc::sansio::Protocol;
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use std::sync::Arc;
use std::time::{Duration, Instant};
use webrtc::media_stream::track_remote::TrackRemote;
use webrtc::peer_connection::RTCIceGatheringState;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Sender, block_on, channel, default_runtime, sleep};

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(30);

struct TrackObservation {
    ssrcs: Vec<u32>,
    codec_mime: Option<String>,
    codings_len: usize,
}

struct WebrtcHandler {
    gather_complete_tx: Sender<()>,
    state_tx: Sender<RTCPeerConnectionState>,
    track_ready_tx: Sender<TrackObservation>,
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

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let ssrcs = track.ssrcs().await;
        let codec_mime = if let Some(ssrc) = ssrcs.first().copied() {
            track.codec(ssrc).await.map(|codec| codec.mime_type)
        } else {
            None
        };
        let codings_len = track.codings().await.len();

        let _ = self.track_ready_tx.try_send(TrackObservation {
            ssrcs,
            codec_mime,
            codings_len,
        });
    }
}

#[test]
fn test_on_track_exposes_ssrcs_and_codec_immediately() {
    block_on(run_test()).unwrap();
}

async fn run_test() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;
    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>(1);
    let (state_tx, mut state_rx) = channel::<RTCPeerConnectionState>(8);
    let (track_ready_tx, mut track_ready_rx) = channel::<TrackObservation>(1);

    let config = RTCConfigurationBuilder::new().build();

    let mut webrtc_media_engine = MediaEngine::default();
    let video_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_VP8.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: String::new(),
            rtcp_feedback: vec![],
        },
        payload_type: 96,
        ..Default::default()
    };
    webrtc_media_engine.register_codec(video_codec.clone(), RtpCodecKind::Video)?;

    let handler = Arc::new(WebrtcHandler {
        gather_complete_tx,
        state_tx,
        track_ready_tx,
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

    let std_socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let rtc_local_addr = std_socket.local_addr()?;
    let rtc_socket = runtime.wrap_udp_socket(std_socket)?;

    let mut setting_engine = SettingEngine::default();
    setting_engine.set_answering_dtls_role(RTCDtlsRole::Server)?;

    let mut rtc_media_engine = MediaEngine::default();
    rtc_media_engine.register_codec(video_codec.clone(), RtpCodecKind::Video)?;
    let registry = register_default_interceptors(Registry::new(), &mut rtc_media_engine)?;

    let mut rtc_pc = RTCPeerConnectionBuilder::new()
        .with_configuration(config)
        .with_setting_engine(setting_engine)
        .with_media_engine(rtc_media_engine)
        .with_interceptor_registry(registry)
        .build()?;

    let test_ssrc = 0x1020_3040;
    let output_track = MediaStreamTrack::new(
        "test-stream".to_string(),
        "video-track".to_string(),
        "video-track".to_string(),
        RtpCodecKind::Video,
        vec![RTCRtpEncodingParameters {
            rtp_coding_parameters: RTCRtpCodingParameters {
                ssrc: Some(test_ssrc),
                ..Default::default()
            },
            codec: video_codec.rtp_codec.clone(),
            ..Default::default()
        }],
    );
    let sender_id = rtc_pc.add_track(output_track)?;

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

    let offer = rtc_pc.create_offer(None)?;
    rtc_pc.set_local_description(offer.clone())?;

    webrtc_pc
        .set_remote_description(rtc::peer_connection::sdp::RTCSessionDescription::offer(
            offer.sdp.clone(),
        )?)
        .await?;

    let answer = webrtc_pc.create_answer(None).await?;
    webrtc_pc.set_local_description(answer).await?;
    let _ = webrtc::runtime::timeout(Duration::from_secs(5), gather_complete_rx.recv()).await;
    let answer_with_cands = webrtc_pc
        .local_description()
        .await
        .expect("local description should be set");
    rtc_pc.set_remote_description(rtc::peer_connection::sdp::RTCSessionDescription::answer(
        answer_with_cands.sdp,
    )?)?;

    let mut buf = vec![0u8; 2000];
    let mut rtc_connected = false;
    let mut webrtc_connected = false;
    let mut sequence_number: u16 = 0;
    let start_time = Instant::now();
    let test_timeout = Duration::from_secs(15);

    loop {
        if start_time.elapsed() > test_timeout {
            anyhow::bail!("timeout waiting for on_track");
        }

        while let Some(msg) = rtc_pc.poll_write() {
            rtc_socket
                .send_to(&msg.message, msg.transport.peer_addr)
                .await?;
        }

        while let Some(event) = rtc_pc.poll_event() {
            match event {
                RTCPeerConnectionEvent::OnIceConnectionStateChangeEvent(state) => {
                    if state == RTCIceConnectionState::Failed {
                        anyhow::bail!("rtc ICE failed");
                    }
                }
                RTCPeerConnectionEvent::OnConnectionStateChangeEvent(state) => {
                    if state == RTCPeerConnectionState::Failed {
                        anyhow::bail!("rtc peer connection failed");
                    }
                    if state == RTCPeerConnectionState::Connected {
                        rtc_connected = true;
                    }
                }
                _ => {}
            }
        }

        while let Ok(state) = state_rx.try_recv() {
            if state == RTCPeerConnectionState::Connected {
                webrtc_connected = true;
            }
        }

        if rtc_connected && webrtc_connected {
            let mut rtp_sender = rtc_pc
                .rtp_sender(sender_id)
                .ok_or_else(|| anyhow::anyhow!("missing RTP sender"))?;
            let packet = rtp::packet::Packet {
                header: rtp::header::Header {
                    version: 2,
                    payload_type: 96,
                    sequence_number,
                    timestamp: 0,
                    ssrc: test_ssrc,
                    ..Default::default()
                },
                payload: bytes::Bytes::from_static(&[0x90, 0x90, 0x90, 0x90]),
            };
            let _ = rtp_sender.write_rtp(packet);
            sequence_number = sequence_number.wrapping_add(1);
        }

        if let Ok(observation) = track_ready_rx.try_recv() {
            assert_eq!(observation.ssrcs, vec![test_ssrc]);
            assert_eq!(observation.codec_mime.as_deref(), Some(MIME_TYPE_VP8));
            assert_eq!(observation.codings_len, 1);
            break;
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

        let timer = sleep(delay.min(Duration::from_millis(20)));
        futures::select! {
            _ = timer.fuse() => {
                rtc_pc.handle_timeout(Instant::now())?;
            }
            res = rtc_socket.recv_from(&mut buf).fuse() => {
                let (n, peer_addr) = res?;
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
        }
    }

    webrtc_pc.close().await?;
    rtc_pc.close()?;

    Ok(())
}
