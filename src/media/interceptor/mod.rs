use crate::media::rtp::rtp_codec::{RTPCodecCapability, RTPHeaderExtensionParameter};
use crate::media::rtp::{PayloadType, SSRC};
use crate::media::track::track_local::TrackLocalWriter;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use interceptor::stream_info::{RTCPFeedback, RTPHeaderExtension, StreamInfo};
use interceptor::{Attributes, RTPWriter};
use std::sync::Arc;
use tokio::sync::Mutex;
use util::Unmarshal;

pub(crate) struct InterceptorToTrackLocalWriter {
    pub(crate) interceptor_rtp_writer: Mutex<Option<Arc<dyn RTPWriter + Send + Sync>>>,
}

impl InterceptorToTrackLocalWriter {
    pub(crate) fn new() -> Self {
        InterceptorToTrackLocalWriter {
            interceptor_rtp_writer: Mutex::new(None),
        }
    }
}

impl std::fmt::Debug for InterceptorToTrackLocalWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InterceptorToTrackLocalWriter").finish()
    }
}

impl Default for InterceptorToTrackLocalWriter {
    fn default() -> Self {
        InterceptorToTrackLocalWriter {
            interceptor_rtp_writer: Mutex::new(None),
        }
    }
}

#[async_trait]
impl TrackLocalWriter for InterceptorToTrackLocalWriter {
    async fn write_rtp(&self, pkt: &rtp::packet::Packet) -> Result<usize> {
        let interceptor_rtp_writer = self.interceptor_rtp_writer.lock().await;
        if let Some(writer) = &*interceptor_rtp_writer {
            let a = Attributes::new();
            writer.write(pkt, &a).await
        } else {
            Ok(0)
        }
    }

    async fn write(&self, b: &Bytes) -> Result<usize> {
        let buf = &mut b.clone();
        let pkt = rtp::packet::Packet::unmarshal(buf)?;
        self.write_rtp(&pkt).await
    }
}

pub(crate) fn create_stream_info(
    id: String,
    ssrc: SSRC,
    payload_type: PayloadType,
    codec: RTPCodecCapability,
    webrtc_header_extensions: &[RTPHeaderExtensionParameter],
) -> StreamInfo {
    let mut header_extensions = vec![];
    for h in webrtc_header_extensions {
        header_extensions.push(RTPHeaderExtension {
            id: h.id,
            uri: h.uri.clone(),
        });
    }

    let mut feedbacks = vec![];
    for f in &codec.rtcp_feedback {
        feedbacks.push(RTCPFeedback {
            typ: f.typ.clone(),
            parameter: f.parameter.clone(),
        });
    }

    StreamInfo {
        id,
        attributes: Attributes::new(),
        ssrc,
        payload_type,
        rtp_header_extensions: header_extensions,
        mime_type: codec.mime_type,
        clock_rate: codec.clock_rate,
        channels: codec.channels,
        sdp_fmtp_line: codec.sdp_fmtp_line,
        rtcp_feedback: feedbacks,
    }
}
