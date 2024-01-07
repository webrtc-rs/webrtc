use bytes::Bytes;
use media::Sample;
use tokio::sync::mpsc;
use tokio::time::Duration;
use waitgroup::{WaitGroup, Worker};
use std::future::Future;

use super::*;
use crate::api::media_engine::{MIME_TYPE_OPUS, MIME_TYPE_VP8};
use crate::error::Result;
use crate::peer_connection::peer_connection_state::RTCPeerConnectionState;
use crate::peer_connection::peer_connection_test::{
    StateHandler, close_pair_now, create_vnet_pair, signal_pair,
};
use crate::peer_connection::PeerConnectionEventHandler;
use crate::rtp_transceiver::rtp_codec::RTCRtpHeaderExtensionParameters;
use crate::rtp_transceiver::RTCPFeedback;
use crate::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use crate::track::track_local::TrackLocal;

lazy_static! {
    static ref P: RTCRtpParameters = RTCRtpParameters {
        codecs: vec![RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_string(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_string(),
                rtcp_feedback: vec![RTCPFeedback {
                    typ: "nack".to_owned(),
                    parameter: "".to_owned(),
                }],
            },
            payload_type: 111,
            ..Default::default()
        }],
        header_extensions: vec![
            RTCRtpHeaderExtensionParameters {
                uri: "urn:ietf:params:rtp-hdrext:sdes:mid".to_owned(),
                ..Default::default()
            },
            RTCRtpHeaderExtensionParameters {
                uri: "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id".to_owned(),
                ..Default::default()
            },
            RTCRtpHeaderExtensionParameters {
                uri: "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id".to_owned(),
                ..Default::default()
            },
        ],
    };
}

//use log::LevelFilter;
//use std::io::Write;

#[tokio::test]
async fn test_set_rtp_parameters() -> Result<()> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, LevelFilter::Trace)
    .init();*/

    let (mut sender, mut receiver, wan) = create_vnet_pair().await?;

    let outgoing_track: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    sender.add_track(Arc::clone(&outgoing_track)).await?;

    // Those parameters wouldn't make sense in a real application,
    // but for the sake of the test we just need different values.

    let (seen_packet_tx, mut seen_packet_rx) = mpsc::channel::<()>(1);
    let seen_packet_tx = Arc::new(Mutex::new(Some(seen_packet_tx)));

    struct TrackHandler {
        seen_packet_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
        worker: Arc<Mutex<Option<Worker>>>,
    }

    impl PeerConnectionEventHandler for TrackHandler {
        fn on_track(&mut self, _: _, receiver: Arc<RTCRtpReceiver>, _: _) -> impl Future<Output = ()> + Send {
            receiver.set_rtp_parameters(P.clone()).await;
            if let Some(track) = receiver.track().await {
                let incoming_track_codecs = track.codec();
                assert_eq!(P.header_extensions, track.params().header_extensions);
                assert_eq!(
                    P.codecs[0].capability.mime_type,
                    incoming_track_codecs.capability.mime_type
                );
                assert_eq!(
                    P.codecs[0].capability.clock_rate,
                    incoming_track_codecs.capability.clock_rate
                );
                assert_eq!(
                    P.codecs[0].capability.channels,
                    incoming_track_codecs.capability.channels
                );
                assert_eq!(
                    P.codecs[0].capability.sdp_fmtp_line,
                    incoming_track_codecs.capability.sdp_fmtp_line
                );
                assert_eq!(
                    P.codecs[0].capability.rtcp_feedback,
                    incoming_track_codecs.capability.rtcp_feedback
                );
                assert_eq!(P.codecs[0].payload_type, incoming_track_codecs.payload_type);

                let done = self.seen_packet_tx.lock().await;
                done.take();
            }
        }

        fn on_peer_connection_state_change(&mut self, state: RTCPeerConnectionState) -> impl Future<Output = ()> + Send {
            if state == RTCPeerConnectionState::Connected {
                let mut worker = self.worker.lock().await;
                worker.take();
            }
        }
    }

    let wg = WaitGroup::new();
    receiver.with_event_handler( TrackHandler { seen_packet_tx: seen_packet_tx.clone(), worker: Arc::new(Mutex::new(Some(wg.worker())))});
    sender.with_event_handler(StateHandler{worker: Arc::new(Mutex::new(Some(wg.worker())))});

    signal_pair(&mut sender, &mut receiver).await?;

    wg.wait().await;

    if let Some(v) = outgoing_track
        .as_any()
        .downcast_ref::<TrackLocalStaticSample>()
    {
        v.write_sample(&Sample {
            data: Bytes::from_static(&[0xAA]),
            duration: Duration::from_secs(1),
            ..Default::default()
        })
        .await?;
    } else {
        panic!();
    }

    let _ = seen_packet_rx.recv().await;
    {
        let mut w = wan.lock().await;
        w.stop().await?;
    }
    close_pair_now(&sender, &receiver).await;

    Ok(())
}

// Assert that SetReadDeadline works as expected
// This test uses VNet since we must have zero loss
#[tokio::test]
async fn test_rtp_receiver_set_read_deadline() -> Result<()> {
    let (mut sender, mut receiver, wan) = create_vnet_pair().await?;

    let track: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    sender.add_track(Arc::clone(&track)).await?;

    let (seen_packet_tx, mut seen_packet_rx) = mpsc::channel::<()>(1);
    let seen_packet_tx = Arc::new(Mutex::new(Some(seen_packet_tx)));

    struct TrackHandler {
        seen_packet_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
        worker: Arc<Mutex<Option<Worker>>>,
    }

    impl PeerConnectionEventHandler for TrackHandler {
        fn on_track(&mut self, track: Arc<TrackRemote>, receiver: Arc<RTCRtpReceiver>, _: _) -> impl Future<Output = ()> + Send {
            // First call will not error because we cache for probing
            let result = tokio::time::timeout(Duration::from_secs(1), track.read_rtp()).await;
            assert!(
                result.is_ok(),
                " First call will not error because we cache for probing"
            );

            let result = tokio::time::timeout(Duration::from_secs(1), track.read_rtp()).await;
            assert!(result.is_err());

            let result = tokio::time::timeout(Duration::from_secs(1), receiver.read_rtcp()).await;
            assert!(result.is_err());

            let mut done = self.seen_packet_tx.lock().await;
            done.take();
        }
        
        fn on_peer_connection_state_change(&mut self, state: RTCPeerConnectionState) -> impl Future<Output = ()> + Send {
            if state == RTCPeerConnectionState::Connected {
                let mut worker = self.worker.lock().await;
                worker.take();
            }
        }
    }

    let wg = WaitGroup::new();
    receiver.with_event_handler(TrackHandler{ seen_packet_tx: seen_packet_tx.clone(), worker: Arc::new(Mutex::new(Some(wg.worker())))});

    sender.with_event_handler(StateHandler{worker: Arc::new(Mutex::new(Some(wg.worker())))});

    signal_pair(&mut sender, &mut receiver).await?;

    wg.wait().await;

    if let Some(v) = track.as_any().downcast_ref::<TrackLocalStaticSample>() {
        v.write_sample(&Sample {
            data: Bytes::from_static(&[0xAA]),
            duration: Duration::from_secs(1),
            ..Default::default()
        })
        .await?;
    } else {
        panic!();
    }

    let _ = seen_packet_rx.recv().await;
    {
        let mut w = wan.lock().await;
        w.stop().await?;
    }
    close_pair_now(&sender, &receiver).await;

    Ok(())
}
