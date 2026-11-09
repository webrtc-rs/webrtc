//! Integration test for `TrackLocalStaticRTP::write_rtp_with_extensions`.
//!
//! A track added before the remote offer is applied is bound while no header extension is
//! negotiated yet, so the ids known at that point are the media engine's provisional ones. The
//! offer then fixes the real ids. Extensions written afterwards must carry the negotiated id:
//! a packet carrying a provisional id is either refused by the sender or misread by the remote.
//!
//! Topology: two async webrtc peers. The offerer only receives video and registers the
//! playout-delay extension after the default interceptors' extensions; the answerer adds its
//! track before applying the offer and registers playout-delay first, so its provisional id for
//! it differs from the negotiated one.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::interceptor_registry::{
    Registry, register_default_interceptors,
};
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_VP8, MediaEngine};
use rtc::rtp::extension::HeaderExtension;
use rtc::rtp::extension::playout_delay_extension::PlayoutDelayExtension;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RTCRtpHeaderExtensionCapability, RtpCodecKind,
};
use rtc::shared::marshal::Unmarshal;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
};
use webrtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
use webrtc::runtime::{Runtime, Sender, channel};

mod common;
use common::{block_on, interval, runtime, timeout};

const PLAYOUT_DELAY_URI: &str = "http://www.webrtc.org/experiments/rtp-hdrext/playout-delay";
const VIDEO_SSRC: u32 = 0x0BAD_CAFE;
const PAYLOAD_TYPE: u8 = 96;

struct Handler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    packets_tx: Option<Sender<rtc::rtp::Packet>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let Some(packets_tx) = self.packets_tx.clone() else {
            return;
        };
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = track.poll().await {
                match event {
                    TrackRemoteEvent::OnRtpPacket(packet) => {
                        let _ = packets_tx.try_send(packet);
                    }
                    TrackRemoteEvent::OnEnded => break,
                    _ => {}
                }
            }
        }));
    }
}

fn vp8() -> RTCRtpCodec {
    RTCRtpCodec {
        mime_type: MIME_TYPE_VP8.to_owned(),
        clock_rate: 90000,
        channels: 0,
        sdp_fmtp_line: String::new(),
        rtcp_feedback: vec![],
    }
}

fn register_playout_delay(media_engine: &mut MediaEngine) {
    media_engine
        .register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: PLAYOUT_DELAY_URI.to_owned(),
            },
            RtpCodecKind::Video,
            None,
        )
        .expect("register playout-delay");
}

async fn build_peer(
    runtime: Arc<dyn Runtime>,
    handler: Handler,
    playout_delay_first: bool,
) -> Arc<dyn PeerConnection> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_codec(
            RTCRtpCodecParameters {
                rtp_codec: vp8(),
                payload_type: PAYLOAD_TYPE,
            },
            RtpCodecKind::Video,
        )
        .expect("register VP8");
    if playout_delay_first {
        register_playout_delay(&mut media_engine);
    }
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .expect("default interceptors");
    if !playout_delay_first {
        register_playout_delay(&mut media_engine);
    }

    Arc::new(
        PeerConnectionBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .with_handler(Arc::new(handler))
            .with_runtime(runtime)
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await
            .expect("build peer connection"),
    )
}

/// The id the session description maps the playout-delay extension to.
fn playout_delay_id(description: &webrtc::peer_connection::RTCSessionDescription) -> u8 {
    let parsed = description.unmarshal().expect("parse SDP");
    parsed
        .media_descriptions
        .iter()
        .flat_map(|media| media.attributes.iter())
        .filter(|attribute| attribute.key == "extmap")
        .filter_map(|attribute| attribute.value.as_deref())
        .find(|value| value.ends_with(PLAYOUT_DELAY_URI))
        .and_then(|value| value.split([' ', '/']).next())
        .and_then(|id| id.parse().ok())
        .expect("playout-delay negotiated")
}

#[test]
fn test_write_rtp_with_extensions_uses_negotiated_ids() {
    block_on(async {
        let runtime = runtime();

        let (offerer_gathered_tx, mut offerer_gathered) = channel::<()>(1);
        let (packets_tx, mut packets) = channel::<rtc::rtp::Packet>(64);
        let offerer = build_peer(
            runtime.clone(),
            Handler {
                runtime: runtime.clone(),
                gather_complete_tx: offerer_gathered_tx,
                packets_tx: Some(packets_tx),
            },
            false,
        )
        .await;
        offerer
            .add_transceiver_from_kind(
                RtpCodecKind::Video,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Recvonly,
                    streams: vec![],
                    send_encodings: vec![],
                }),
            )
            .await
            .expect("add recvonly transceiver");

        let (answerer_gathered_tx, mut answerer_gathered) = channel::<()>(1);
        let answerer = build_peer(
            runtime.clone(),
            Handler {
                runtime: runtime.clone(),
                gather_complete_tx: answerer_gathered_tx,
                packets_tx: None,
            },
            true,
        )
        .await;
        let track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
            "stream".to_owned(),
            "video".to_owned(),
            "video".to_owned(),
            RtpCodecKind::Video,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(VIDEO_SSRC),
                    ..Default::default()
                },
                codec: vp8(),
                ..Default::default()
            }],
        )));
        // Before the offer is applied, as an answering server typically does.
        answerer
            .add_track(Arc::clone(&track) as Arc<dyn TrackLocal>)
            .await
            .expect("add track");

        let offer = offerer.create_offer(None).await.expect("create offer");
        offerer
            .set_local_description(offer)
            .await
            .expect("set offer");
        timeout(Duration::from_secs(5), offerer_gathered.recv())
            .await
            .expect("offerer gathering");
        let offer = offerer.local_description().await.expect("offer");
        answerer
            .set_remote_description(offer)
            .await
            .expect("apply offer");
        let answer = answerer.create_answer(None).await.expect("create answer");
        answerer
            .set_local_description(answer)
            .await
            .expect("set answer");
        timeout(Duration::from_secs(5), answerer_gathered.recv())
            .await
            .expect("answerer gathering");
        let answer = answerer.local_description().await.expect("answer");
        let negotiated_id = playout_delay_id(&answer);
        offerer
            .set_remote_description(answer)
            .await
            .expect("apply answer");

        let delay = PlayoutDelayExtension::new(1, 50);
        let writer = Arc::clone(&track);
        runtime.spawn(Box::pin(async move {
            let mut ticker = interval(Duration::from_millis(20));
            let mut sequence_number: u16 = 0;
            loop {
                let _ = ticker.tick().await;
                let packet = rtc::rtp::Packet {
                    header: rtc::rtp::Header {
                        version: 2,
                        marker: true,
                        payload_type: PAYLOAD_TYPE,
                        sequence_number,
                        timestamp: u32::from(sequence_number) * 3000,
                        ssrc: VIDEO_SSRC,
                        ..Default::default()
                    },
                    // A VP8 payload descriptor (start of partition) and one byte.
                    payload: Bytes::from_static(&[0x10, 0x00]),
                };
                sequence_number = sequence_number.wrapping_add(1);
                if writer
                    .write_rtp_with_extensions(packet, &[HeaderExtension::PlayoutDelay(delay)])
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }));

        let packet = timeout(Duration::from_secs(10), packets.recv())
            .await
            .expect("an RTP packet reaches the remote")
            .expect("packet channel open");
        let payload = packet
            .header
            .get_extension(negotiated_id)
            .expect("playout-delay at its negotiated id");
        assert_eq!(
            PlayoutDelayExtension::unmarshal(&mut payload.as_ref()).expect("playout-delay value"),
            delay
        );

        offerer.close().await.expect("close offerer");
        answerer.close().await.expect("close answerer");
    });
}
