#[cfg(test)]
mod track_local_static_test;

pub mod track_local_static_rtp;
pub mod track_local_static_sample;

use crate::error::{Error, Result};
use crate::rtp_transceiver::rtp_codec::*;
use crate::rtp_transceiver::*;

use async_trait::async_trait;
use interceptor::{Attributes, RTCPReader, RTPWriter};
use std::any::Any;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use util::Unmarshal;

/// TrackLocalWriter is the Writer for outbound RTP Packets
#[async_trait]
pub trait TrackLocalWriter: fmt::Debug {
    /// write_rtp encrypts a RTP packet and writes to the connection
    async fn write_rtp(&self, p: &rtp::packet::Packet) -> Result<usize>;

    /// write encrypts and writes a full RTP packet
    async fn write(&self, b: &[u8]) -> Result<usize>;
}

/// TrackLocalContext is the Context passed when a TrackLocal has been Binded/Unbinded from a PeerConnection, and used
/// in Interceptors.
#[derive(Default, Clone)]
pub struct TrackLocalContext {
    pub(crate) id: String,
    pub(crate) params: RTCRtpParameters,
    pub(crate) ssrc: SSRC,
    pub(crate) write_stream: Option<Arc<dyn TrackLocalWriter + Send + Sync>>,
    pub(crate) rtcp_intercepter: Option<Arc<dyn RTCPReader + Send + Sync>>,
}

impl TrackLocalContext {
    /// codec_parameters returns the negotiated RTPCodecParameters. These are the codecs supported by both
    /// PeerConnections and the SSRC/PayloadTypes
    pub fn codec_parameters(&self) -> &[RTCRtpCodecParameters] {
        &self.params.codecs
    }

    /// header_extensions returns the negotiated RTPHeaderExtensionParameters. These are the header extensions supported by
    /// both PeerConnections and the SSRC/PayloadTypes
    pub fn header_extensions(&self) -> &[RTCRtpHeaderExtensionParameters] {
        &self.params.header_extensions
    }

    /// ssrc requires the negotiated SSRC of this track
    /// This track may have multiple if RTX is enabled
    pub fn ssrc(&self) -> SSRC {
        self.ssrc
    }

    /// write_stream returns the write_stream for this TrackLocal. The implementer writes the outbound
    /// media packets to it
    pub fn write_stream(&self) -> Option<Arc<dyn TrackLocalWriter + Send + Sync>> {
        self.write_stream.clone()
    }

    pub fn rtcp_reader(&self) -> Option<Arc<dyn RTCPReader + Send + Sync>> {
        self.rtcp_intercepter.clone()
    }

    /// id is a unique identifier that is used for both bind/unbind
    pub fn id(&self) -> String {
        self.id.clone()
    }
}
/// TrackLocal is an interface that controls how the user can send media
/// The user can provide their own TrackLocal implementatiosn, or use
/// the implementations in pkg/media
#[async_trait]
pub trait TrackLocal {
    /// bind should implement the way how the media data flows from the Track to the PeerConnection
    /// This will be called internally after signaling is complete and the list of available
    /// codecs has been determined
    async fn bind(&self, t: &TrackLocalContext) -> Result<RTCRtpCodecParameters>;

    /// unbind should implement the teardown logic when the track is no longer needed. This happens
    /// because a track has been stopped.
    async fn unbind(&self, t: &TrackLocalContext) -> Result<()>;

    /// id is the unique identifier for this Track. This should be unique for the
    /// stream, but doesn't have to globally unique. A common example would be 'audio' or 'video'
    /// and stream_id would be 'desktop' or 'webcam'
    fn id(&self) -> &str;

    /// rid is the RTP identifier for this Track
    fn rid(&self) -> &str;

    /// stream_id is the group this track belongs too. This must be unique
    fn stream_id(&self) -> &str;

    /// kind controls if this TrackLocal is audio or video
    fn kind(&self) -> RTPCodecType;

    fn as_any(&self) -> &dyn Any;
}

/// TrackBinding is a single bind for a Track
/// Bind can be called multiple times, this stores the
/// result for a single bind call so that it can be used when writing
#[derive(Default, Debug)]
pub(crate) struct TrackBinding {
    id: String,
    ssrc: SSRC,
    payload_type: PayloadType,
    write_stream: Option<Arc<dyn TrackLocalWriter + Send + Sync>>,
}

pub(crate) struct InterceptorToTrackLocalWriter {
    pub(crate) interceptor_rtp_writer: Mutex<Option<Arc<dyn RTPWriter + Send + Sync>>>,
    paused: Arc<AtomicBool>,
}

impl InterceptorToTrackLocalWriter {
    pub(crate) fn new(paused: Arc<AtomicBool>) -> Self {
        InterceptorToTrackLocalWriter {
            interceptor_rtp_writer: Mutex::new(None),
            paused,
        }
    }
}

impl std::fmt::Debug for InterceptorToTrackLocalWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InterceptorToTrackLocalWriter").finish()
    }
}

#[async_trait]
impl TrackLocalWriter for InterceptorToTrackLocalWriter {
    async fn write_rtp(&self, pkt: &rtp::packet::Packet) -> Result<usize> {
        if self.paused.load(Ordering::SeqCst) {
            return Ok(0);
        }

        let interceptor_rtp_writer = self.interceptor_rtp_writer.lock().await;
        if let Some(writer) = &*interceptor_rtp_writer {
            let a = Attributes::new();
            Ok(writer.write(pkt, &a).await?)
        } else {
            Ok(0)
        }
    }

    async fn write(&self, mut b: &[u8]) -> Result<usize> {
        let pkt = rtp::packet::Packet::unmarshal(&mut b)?;
        self.write_rtp(&pkt).await
    }
}
