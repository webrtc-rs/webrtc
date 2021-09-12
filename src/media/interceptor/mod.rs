use crate::media::rtp::rtp_codec::{RTPCodecCapability, RTPHeaderExtensionParameter};
use crate::media::rtp::{PayloadType, SSRC};
use crate::media::track::track_local::TrackLocalWriter;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use interceptor::stream_info::{RTCPFeedback, RTPHeaderExtension, StreamInfo};
use interceptor::Attributes;
use util::Unmarshal;

#[derive(Debug, Clone)]
pub(crate) struct InterceptorToTrackLocalWriter {
    // interceptor atomic.Value //  // interceptor.RTPWriter }
}

#[async_trait]
impl TrackLocalWriter for InterceptorToTrackLocalWriter {
    async fn write_rtp(&self, _p: &rtp::packet::Packet) -> Result<usize> {
        /*TODO:
           if writer, ok := i.interceptor.Load().(interceptor.RTPWriter); ok && writer != nil {
            return writer.Write(header, payload, interceptor.Attributes{})
        }

        return 0, nil*/
        Ok(0)
    }

    async fn write(&self, b: &Bytes) -> Result<usize> {
        let buf = &mut b.clone();
        let packet = rtp::packet::Packet::unmarshal(buf)?;
        self.write_rtp(&packet).await
    }

    fn clone_to(&self) -> Box<dyn TrackLocalWriter + Send + Sync> {
        Box::new(self.clone())
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
