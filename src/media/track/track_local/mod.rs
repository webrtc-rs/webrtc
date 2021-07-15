pub mod track_local_static_rtp;
pub mod track_local_static_sample;

use crate::error::Error;
use crate::media::rtp::rtp_codec::*;
use crate::media::rtp::*;

use anyhow::Result;
use bytes::Bytes;
use std::fmt;
use util::Unmarshal;

/// TrackLocalWriter is the Writer for outbound RTP Packets
pub trait TrackLocalWriter: fmt::Debug {
    /// write_rtp encrypts a RTP packet and writes to the connection
    fn write_rtp(&self, p: &rtp::packet::Packet) -> Result<usize>;

    /// write encrypts and writes a full RTP packet
    fn write(&self, b: &Bytes) -> Result<usize>;

    fn clone_to(&self) -> Box<dyn TrackLocalWriter + Send + Sync>;
}

impl Clone for Box<dyn TrackLocalWriter + Send + Sync> {
    fn clone(&self) -> Box<dyn TrackLocalWriter + Send + Sync> {
        self.clone_to()
    }
}

/// TrackLocalContext is the Context passed when a TrackLocal has been Binded/Unbinded from a PeerConnection, and used
/// in Interceptors.
#[derive(Debug, Clone)]
pub struct TrackLocalContext {
    id: String,
    params: RTPParameters,
    ssrc: SSRC,
    write_stream: Box<dyn TrackLocalWriter + Send + Sync>,
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
    pub fn write_stream(&self) -> Box<dyn TrackLocalWriter + Send + Sync> {
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
pub trait TrackLocal {
    /// bind should implement the way how the media data flows from the Track to the PeerConnection
    /// This will be called internally after signaling is complete and the list of available
    /// codecs has been determined
    fn bind(&mut self, t: TrackLocalContext) -> Result<RTPCodecParameters>;

    /// unbind should implement the teardown logic when the track is no longer needed. This happens
    /// because a track has been stopped.
    fn unbind(&mut self, t: TrackLocalContext) -> Result<()>;

    /// id is the unique identifier for this Track. This should be unique for the
    /// stream, but doesn't have to globally unique. A common example would be 'audio' or 'video'
    /// and stream_id would be 'desktop' or 'webcam'
    fn id(&self) -> String;

    /// stream_id is the group this track belongs too. This must be unique
    fn stream_id(&self) -> String;

    /// kind controls if this TrackLocal is audio or video
    fn kind(&self) -> RTPCodecType;
}

/// TrackBinding is a single bind for a Track
/// Bind can be called multiple times, this stores the
/// result for a single bind call so that it can be used when writing
#[derive(Debug, Clone)]
pub(crate) struct TrackBinding {
    id: String,
    ssrc: SSRC,
    payload_type: PayloadType,
    write_stream: Box<dyn TrackLocalWriter + Send + Sync>,
}
