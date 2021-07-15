use super::*;

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Debug, Clone)]
pub struct TrackLocalStaticRTP {
    bindings: Vec<TrackBinding>,
    codec: RTPCodecCapability,
    id: String,
    stream_id: String,
}

impl TrackLocalStaticRTP {
    /// returns a TrackLocalStaticRTP.
    pub fn new(codec: RTPCodecCapability, id: String, stream_id: String) -> Self {
        TrackLocalStaticRTP {
            codec,
            bindings: vec![],
            id,
            stream_id,
        }
    }

    /// codec gets the Codec of the track
    pub fn codec(&self) -> RTPCodecCapability {
        self.codec.clone()
    }
}

impl TrackLocal for TrackLocalStaticRTP {
    /// bind is called by the PeerConnection after negotiation is complete
    /// This asserts that the code requested is supported by the remote peer.
    /// If so it setups all the state (SSRC and PayloadType) to have a call
    fn bind(&mut self, t: TrackLocalContext) -> Result<RTPCodecParameters> {
        let parameters = RTPCodecParameters {
            capability: self.codec.clone(),
            ..Default::default()
        };

        let (codec, match_type) = codec_parameters_fuzzy_search(&parameters, t.codec_parameters());
        if match_type != CodecMatchType::None {
            self.bindings.push(TrackBinding {
                ssrc: t.ssrc(),
                payload_type: codec.payload_type,
                write_stream: t.write_stream(),
                id: t.id(),
            });

            Ok(codec)
        } else {
            Err(Error::ErrUnsupportedCodec.into())
        }
    }

    /// unbind implements the teardown logic when the track is no longer needed. This happens
    /// because a track has been stopped.
    fn unbind(&mut self, t: TrackLocalContext) -> Result<()> {
        let mut idx = None;
        for (index, binding) in self.bindings.iter().enumerate() {
            if binding.id == t.id() {
                idx = Some(index);
                break;
            }
        }
        if let Some(index) = idx {
            self.bindings.remove(index);
            Ok(())
        } else {
            Err(Error::ErrUnbindFailed.into())
        }
    }

    /// id is the unique identifier for this Track. This should be unique for the
    /// stream, but doesn't have to globally unique. A common example would be 'audio' or 'video'
    /// and StreamID would be 'desktop' or 'webcam'
    fn id(&self) -> String {
        self.id.clone()
    }

    /// stream_id is the group this track belongs too. This must be unique
    fn stream_id(&self) -> String {
        self.stream_id.clone()
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
}

impl TrackLocalWriter for TrackLocalStaticRTP {
    /// write_rtp writes a RTP Packet to the TrackLocalStaticRTP
    /// If one PeerConnection fails the packets will still be sent to
    /// all PeerConnections. The error message will contain the ID of the failed
    /// PeerConnections so you can remove them
    fn write_rtp(&self, p: &rtp::packet::Packet) -> Result<usize> {
        let mut n = 0;
        let mut write_err = None;
        let mut pkt = p.clone();

        for b in &self.bindings {
            pkt.header.ssrc = b.ssrc;
            pkt.header.payload_type = b.payload_type;
            match b.write_stream.write_rtp(&pkt) {
                Ok(m) => {
                    n += m;
                }
                Err(err) => {
                    write_err = Some(err);
                }
            }
        }

        if let Some(err) = write_err {
            Err(err)
        } else {
            Ok(n)
        }
    }

    /// write writes a RTP Packet as a buffer to the TrackLocalStaticRTP
    /// If one PeerConnection fails the packets will still be sent to
    /// all PeerConnections. The error message will contain the ID of the failed
    /// PeerConnections so you can remove them
    fn write(&self, b: &Bytes) -> Result<usize> {
        let buf = &mut b.clone();
        let pkt = rtp::packet::Packet::unmarshal(buf)?;
        self.write_rtp(&pkt)?;
        Ok(b.len())
    }

    fn clone_to(&self) -> Box<dyn TrackLocalWriter + Send + Sync> {
        Box::new(self.clone())
    }
}
