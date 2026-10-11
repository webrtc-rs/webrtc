//! Regression test for webrtc#906: a connection that ever held a transceiver must be freed after
//! `close()` and drop.
//!
//! `PeerConnectionRef.rtp_transceivers` owns every `RtpTransceiverImpl`, and each transceiver owns
//! its `RtpSenderImpl` and `RtpReceiverImpl`. All three used to hold a strong
//! `Arc<PeerConnectionRef>` back, a cycle that nothing broke: neither `close()` nor `Drop` empties
//! the map. A single `add_track()` therefore kept the event handler, the sans-I/O core and every
//! buffer alive for the life of the process. The back-references are now `Weak`.
//!
//! The connection is observed through a `Weak` to its event handler, which only the connection
//! holds once the test has handed it to the builder.
use std::sync::Arc;
use std::time::Duration;

use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::MediaEngine;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};

mod common;
use common::{block_on, sleep};

/// How long a closed and dropped connection may take to be freed.
const RELEASE_TIMEOUT: Duration = Duration::from_secs(2);

struct NoopHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for NoopHandler {}

fn video_track() -> Arc<dyn TrackLocal> {
    Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
        "stream".to_owned(),
        "track".to_owned(),
        "label".to_owned(),
        RtpCodecKind::Video,
        vec![RTCRtpEncodingParameters {
            rtp_coding_parameters: RTCRtpCodingParameters {
                ssrc: Some(1234),
                ..Default::default()
            },
            codec: RTCRtpCodec {
                mime_type: "video/VP8".to_owned(),
                clock_rate: 90000,
                ..Default::default()
            },
            ..Default::default()
        }],
    )))
}

/// Builds a connection, optionally adds a track (which creates a transceiver with a sender),
/// closes and drops it, and reports whether its event handler was freed within
/// [`RELEASE_TIMEOUT`].
async fn is_released_after_close_and_drop(add_track: bool, dedicated_reactor: bool) -> bool {
    let handler = Arc::new(NoopHandler);
    let handler_weak = Arc::downgrade(&handler);

    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("register default codecs");
    let mut builder = PeerConnectionBuilder::new()
        .with_media_engine(media_engine)
        .with_handler(handler)
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()]);
    if dedicated_reactor {
        builder = builder.with_dedicated_reactor_pool_size(1);
    }
    let pc = builder.build().await.expect("build peer connection");

    if add_track {
        pc.add_track(video_track()).await.expect("add_track");
    }
    pc.close().await.expect("close");
    drop(pc);

    let step = Duration::from_millis(10);
    let mut waited = Duration::ZERO;
    while waited < RELEASE_TIMEOUT {
        if handler_weak.upgrade().is_none() {
            return true;
        }
        sleep(step).await;
        waited += step;
    }
    false
}

#[test]
fn close_and_drop_release_a_connection_that_added_a_track() {
    block_on(async {
        for dedicated_reactor in [false, true] {
            // Control: without a transceiver the connection was always freed, so a failure below
            // can only come from the transceiver path.
            assert!(
                is_released_after_close_and_drop(false, dedicated_reactor).await,
                "a connection without tracks was not freed (dedicated_reactor={dedicated_reactor})"
            );
            assert!(
                is_released_after_close_and_drop(true, dedicated_reactor).await,
                "a connection that added a track was never freed after close() and drop \
                 (dedicated_reactor={dedicated_reactor}): the transceiver, sender or receiver \
                 still holds a strong reference to it"
            );
        }
    });
}
