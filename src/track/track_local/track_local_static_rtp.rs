use super::*;

use crate::util::flatten_errs;
use std::sync::Arc;
use tokio::sync::Mutex;

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Debug, Clone)]
pub struct TrackLocalStaticRTP {
    pub(crate) bindings: Arc<Mutex<Vec<TrackBinding>>>,
    codec: RTCRtpCodecCapability,
    id: String,
    stream_id: String,
}

impl TrackLocalStaticRTP {
    /// returns a TrackLocalStaticRTP.
    pub fn new(codec: RTCRtpCodecCapability, id: String, stream_id: String) -> Self {
        TrackLocalStaticRTP {
            codec,
            bindings: Arc::new(Mutex::new(vec![])),
            id,
            stream_id,
        }
    }

    /// codec gets the Codec of the track
    pub fn codec(&self) -> RTCRtpCodecCapability {
        self.codec.clone()
    }
}

#[async_trait]
impl TrackLocal for TrackLocalStaticRTP {
    /// bind is called by the PeerConnection after negotiation is complete
    /// This asserts that the code requested is supported by the remote peer.
    /// If so it setups all the state (SSRC and PayloadType) to have a call
    async fn bind(&self, t: &TrackLocalContext) -> Result<RTCRtpCodecParameters> {
        let parameters = RTCRtpCodecParameters {
            capability: self.codec.clone(),
            ..Default::default()
        };

        let (codec, match_type) = codec_parameters_fuzzy_search(&parameters, t.codec_parameters());
        if match_type != CodecMatch::None {
            {
                let mut bindings = self.bindings.lock().await;
                bindings.push(TrackBinding {
                    ssrc: t.ssrc(),
                    payload_type: codec.payload_type,
                    write_stream: t.write_stream(),
                    id: t.id(),
                });
            }

            Ok(codec)
        } else {
            Err(Error::ErrUnsupportedCodec)
        }
    }

    /// unbind implements the teardown logic when the track is no longer needed. This happens
    /// because a track has been stopped.
    async fn unbind(&self, t: &TrackLocalContext) -> Result<()> {
        let mut bindings = self.bindings.lock().await;
        let mut idx = None;
        for (index, binding) in bindings.iter().enumerate() {
            if binding.id == t.id() {
                idx = Some(index);
                break;
            }
        }
        if let Some(index) = idx {
            bindings.remove(index);
            Ok(())
        } else {
            Err(Error::ErrUnbindFailed)
        }
    }

    /// id is the unique identifier for this Track. This should be unique for the
    /// stream, but doesn't have to globally unique. A common example would be 'audio' or 'video'
    /// and StreamID would be 'desktop' or 'webcam'
    fn id(&self) -> &str {
        self.id.as_str()
    }

    /// stream_id is the group this track belongs too. This must be unique
    fn stream_id(&self) -> &str {
        self.stream_id.as_str()
    }

    /// kind controls if this TrackLocal is audio or video
    fn kind(&self) -> RTPCodecType {
        if self.codec.mime_type.starts_with("audio/") {
            RTPCodecType::Audio
        } else if self.codec.mime_type.starts_with("video/") {
            RTPCodecType::Video
        } else {
            RTPCodecType::Unspecified
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[async_trait]
impl TrackLocalWriter for TrackLocalStaticRTP {
    /// write_rtp writes a RTP Packet to the TrackLocalStaticRTP
    /// If one PeerConnection fails the packets will still be sent to
    /// all PeerConnections. The error message will contain the ID of the failed
    /// PeerConnections so you can remove them
    async fn write_rtp(&self, p: &rtp::packet::Packet) -> Result<usize> {
        let mut n = 0;
        let mut write_errs = vec![];
        let mut pkt = p.clone();

        let bindings = self.bindings.lock().await;
        for b in &*bindings {
            pkt.header.ssrc = b.ssrc;
            pkt.header.payload_type = b.payload_type;
            if let Some(write_stream) = &b.write_stream {
                match write_stream.write_rtp(&pkt).await {
                    Ok(m) => {
                        n += m;
                    }
                    Err(err) => {
                        write_errs.push(err);
                    }
                }
            } else {
                write_errs.push(Error::new("track binding has none write_stream".to_owned()));
            }
        }

        flatten_errs(write_errs)?;
        Ok(n)
    }

    /// write writes a RTP Packet as a buffer to the TrackLocalStaticRTP
    /// If one PeerConnection fails the packets will still be sent to
    /// all PeerConnections. The error message will contain the ID of the failed
    /// PeerConnections so you can remove them
    async fn write(&self, mut b: &[u8]) -> Result<usize> {
        let pkt = rtp::packet::Packet::unmarshal(&mut b)?;
        self.write_rtp(&pkt).await?;
        Ok(b.len())
    }
}
