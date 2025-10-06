use super::*;
use std::any::Any;

/// TrackLocalSimple is simples mock of TrackLocal
#[derive(Debug)]
pub struct TrackLocalSimple {
    kind: RTPCodecType,
    id: String,
    stream_id: String,
    ssrc: u32,
}

impl TrackLocalSimple {
    /// returns a TrackLocalStaticRTP without rid.
    pub fn new(kind: RTPCodecType, id: String, stream_id: String, ssrc: u32) -> Self {
        TrackLocalSimple {
            kind,
            id,
            stream_id,
            ssrc,
        }
    }
}

#[async_trait]
impl TrackLocal for TrackLocalSimple {
    async fn bind(&self, _t: &TrackLocalContext) -> Result<RTCRtpCodecParameters> {
        println!(
            "TrackLocalSimple.bind: mid - {:?}; {:?}",
            _t.mid(),
            _t.ssrc()
        );
        Ok(RTCRtpCodecParameters {
            ..Default::default()
        })
    }

    async fn unbind(&self, _t: &TrackLocalContext) -> Result<()> {
        println!(
            "TrackLocalSimple.unbind: mid-{:?}; {:?}",
            _t.mid(),
            _t.ssrc()
        );
        Ok(())
    }

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn rid(&self) -> Option<&str> {
        None
    }

    fn stream_id(&self) -> &str {
        self.stream_id.as_str()
    }

    /// kind controls if this TrackLocal is audio or video
    fn kind(&self) -> RTPCodecType {
        self.kind.clone()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
