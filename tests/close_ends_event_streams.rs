//! Closing a connection must end every application-facing event stream.
//!
//! `DataChannel::poll`, `TrackRemote::poll` and `TrackLocal::poll` each wait on a channel whose
//! sender the connection keeps. `close()` stopped the driver without ever ending those channels:
//! the driver exits at the top of its loop before delivering another event, and the senders stayed
//! in the connection — which a `DataChannel` handle itself keeps alive — so a task blocked in
//! `poll()` never woke, even after the application dropped the connection.
//!
//! The documented contract is that `poll()` returns `None` once the channel or connection is
//! closed; these tests hold `close()` (and a driver stopped by `Drop`) to it.
use std::sync::Arc;
use std::time::Duration;

use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::MediaEngine;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use webrtc::data_channel::DataChannel;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};

mod common;
use common::{block_on, timeout};

/// How long a stream may take to end once the connection is closed.
const STREAM_END_TIMEOUT: Duration = Duration::from_secs(2);

struct NoopHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for NoopHandler {}

async fn build(dedicated_reactor: bool) -> impl PeerConnection {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("register default codecs");
    let mut builder = PeerConnectionBuilder::new()
        .with_media_engine(media_engine)
        .with_handler(Arc::new(NoopHandler))
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()]);
    if dedicated_reactor {
        builder = builder.with_dedicated_reactor_pool_size(1);
    }
    builder.build().await.expect("build peer connection")
}

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

#[test]
fn data_channel_poll_ends_after_close() {
    block_on(async {
        for dedicated_reactor in [false, true] {
            for drop_connection in [false, true] {
                let pc = build(dedicated_reactor).await;
                let dc: Arc<dyn DataChannel> = pc
                    .create_data_channel("events", None)
                    .await
                    .expect("create data channel");

                pc.close().await.expect("close");
                if drop_connection {
                    drop(pc);
                }

                let event = timeout(STREAM_END_TIMEOUT, dc.poll())
                    .await
                    .unwrap_or_else(|_| {
                        panic!(
                            "DataChannel::poll still blocked after close() \
                         (dedicated_reactor={dedicated_reactor}, drop_connection={drop_connection})"
                        )
                    });
                assert!(
                    event.is_none(),
                    "no event is pending, so the stream must end"
                );
            }
        }
    });
}

#[test]
fn track_local_poll_ends_after_close() {
    block_on(async {
        for dedicated_reactor in [false, true] {
            let pc = build(dedicated_reactor).await;
            let track = video_track();
            pc.add_track(Arc::clone(&track)).await.expect("add_track");

            pc.close().await.expect("close");

            let event = timeout(STREAM_END_TIMEOUT, track.poll())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "TrackLocal::poll still blocked after close() \
                         (dedicated_reactor={dedicated_reactor})"
                    )
                });
            assert!(
                event.is_none(),
                "no RTCP is pending, so the stream must end"
            );
        }
    });
}

/// `Drop` on a dedicated reactor stops the driver without `close()`; the driver's stop path must
/// end the streams too.
#[test]
fn data_channel_poll_ends_when_a_dedicated_reactor_connection_is_dropped() {
    block_on(async {
        let pc = build(true).await;
        let dc: Arc<dyn DataChannel> = pc
            .create_data_channel("events", None)
            .await
            .expect("create data channel");

        drop(pc);

        let event = timeout(STREAM_END_TIMEOUT, dc.poll())
            .await
            .expect("DataChannel::poll still blocked after the driver stopped");
        assert!(
            event.is_none(),
            "no event is pending, so the stream must end"
        );
    });
}
