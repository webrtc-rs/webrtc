pub mod track_local_static_rtp;
pub mod track_local_static_sample;

use crate::error::Error;
use crate::media::rtp::rtp_codec::*;
use crate::media::rtp::*;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use std::fmt;
use util::Unmarshal;

/// TrackLocalWriter is the Writer for outbound RTP Packets
#[async_trait]
pub trait TrackLocalWriter: fmt::Debug {
    /// write_rtp encrypts a RTP packet and writes to the connection
    async fn write_rtp(&self, p: &rtp::packet::Packet) -> Result<usize>;

    /// write encrypts and writes a full RTP packet
    async fn write(&self, b: &Bytes) -> Result<usize>;

    fn clone_to(&self) -> Box<dyn TrackLocalWriter + Send + Sync>;
}

impl Clone for Box<dyn TrackLocalWriter + Send + Sync> {
    fn clone(&self) -> Box<dyn TrackLocalWriter + Send + Sync> {
        self.clone_to()
    }
}

/// TrackLocalContext is the Context passed when a TrackLocal has been Binded/Unbinded from a PeerConnection, and used
/// in Interceptors.
#[derive(Default, Debug, Clone)]
pub struct TrackLocalContext {
    pub(crate) id: String,
    pub(crate) params: RTPParameters,
    pub(crate) ssrc: SSRC,
    pub(crate) write_stream: Option<Box<dyn TrackLocalWriter + Send + Sync>>,
}

impl TrackLocalContext {
    /// codec_parameters returns the negotiated RTPCodecParameters. These are the codecs supported by both
    /// PeerConnections and the SSRC/PayloadTypes
    pub fn codec_parameters(&self) -> &[RTPCodecParameters] {
        &self.params.codecs
    }

    /// header_extensions returns the negotiated RTPHeaderExtensionParameters. These are the header extensions supported by
    /// both PeerConnections and the SSRC/PayloadTypes
    pub fn header_extensions(&self) -> &[RTPHeaderExtensionParameter] {
        &self.params.header_extensions
    }

    /// ssrc requires the negotiated SSRC of this track
    /// This track may have multiple if RTX is enabled
    pub fn ssrc(&self) -> SSRC {
        self.ssrc
    }

    /// write_stream returns the write_stream for this TrackLocal. The implementer writes the outbound
    /// media packets to it
    pub fn write_stream(&self) -> Option<Box<dyn TrackLocalWriter + Send + Sync>> {
        self.write_stream.clone()
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
    async fn bind(&self, t: &TrackLocalContext) -> Result<RTPCodecParameters>;

    /// unbind should implement the teardown logic when the track is no longer needed. This happens
    /// because a track has been stopped.
    async fn unbind(&self, t: &TrackLocalContext) -> Result<()>;

    /// id is the unique identifier for this Track. This should be unique for the
    /// stream, but doesn't have to globally unique. A common example would be 'audio' or 'video'
    /// and stream_id would be 'desktop' or 'webcam'
    fn id(&self) -> &str;

    /// stream_id is the group this track belongs too. This must be unique
    fn stream_id(&self) -> &str;

    /// kind controls if this TrackLocal is audio or video
    fn kind(&self) -> RTPCodecType;
}

/// TrackBinding is a single bind for a Track
/// Bind can be called multiple times, this stores the
/// result for a single bind call so that it can be used when writing
#[derive(Default, Debug, Clone)]
pub(crate) struct TrackBinding {
    id: String,
    ssrc: SSRC,
    payload_type: PayloadType,
    write_stream: Option<Box<dyn TrackLocalWriter + Send + Sync>>,
}
