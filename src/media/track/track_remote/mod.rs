use crate::error::Error;
use crate::media::interceptor::{Attributes, Interceptor};
use crate::media::rtp::rtp_codec::{RTPCodecParameters, RTPCodecType, RTPParameters};
use crate::media::rtp::{PayloadType, SSRC};
use crate::RECEIVE_MTU;

use anyhow::Result;
use bytes::{Bytes, BytesMut};
use std::sync::Arc;
use util::Unmarshal;

/// TrackRemote represents a single inbound source of media
#[derive(Default)]
pub struct TrackRemote {
    pub(crate) id: String,
    pub(crate) stream_id: String,

    payload_type: PayloadType,
    pub(crate) kind: RTPCodecType,
    ssrc: SSRC,
    pub(crate) codec: RTPCodecParameters,
    pub(crate) params: RTPParameters,
    rid: String,

    interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,

    //receiver: Arc<RTPReceiver>,
    peeked: Option<Bytes>,
    peeked_attributes: Option<Attributes>,
}

impl TrackRemote {
    pub(crate) fn new(
        kind: RTPCodecType,
        ssrc: SSRC,
        rid: String,
        //receiver: Arc<RTPReceiver>,
        interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,
    ) -> Self {
        TrackRemote {
            kind,
            ssrc,
            rid,
            //receiver,
            interceptor,
            ..Default::default()
        }
    }

    /// id is the unique identifier for this Track. This should be unique for the
    /// stream, but doesn't have to globally unique. A common example would be 'audio' or 'video'
    /// and StreamID would be 'desktop' or 'webcam'
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    /// rid gets the RTP Stream ID of this Track
    /// With Simulcast you will have multiple tracks with the same ID, but different RID values.
    /// In many cases a TrackRemote will not have an RID, so it is important to assert it is non-zero
    pub fn rid(&self) -> &str {
        self.rid.as_str()
    }

    /// payload_type gets the PayloadType of the track
    pub fn payload_type(&self) -> PayloadType {
        self.payload_type
    }

    /// kind gets the Kind of the track
    pub fn kind(&self) -> RTPCodecType {
        self.kind
    }

    /// stream_id is the group this track belongs too. This must be unique
    pub fn stream_id(&self) -> &str {
        self.stream_id.as_str()
    }

    /// ssrc gets the SSRC of the track
    pub fn ssrc(&self) -> SSRC {
        self.ssrc
    }

    /// msid gets the Msid of the track
    pub fn msid(&self) -> String {
        self.stream_id().to_owned() + " " + self.id()
    }

    /// codec gets the Codec of the track
    pub fn codec(&self) -> &RTPCodecParameters {
        &self.codec
    }

    /// Read reads data from the track.
    pub async fn read(&mut self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        if let (Some(data), Some(attributes)) = (self.peeked.take(), self.peeked_attributes.take())
        {
            // someone else may have stolen our packet when we
            // released the lock.  Deal with it.
            let n = std::cmp::min(b.len(), data.len());
            b[..n].copy_from_slice(&data[..n]);
            Ok((n, attributes))
        } else {
            //TODO: self.receiver.read_rtp(b, t)
            Err(Error::new("TODO".to_owned()).into())
        }
    }
    /*
        // ReadRTP is a convenience method that wraps Read and unmarshals for you.
        func (t *TrackRemote) ReadRTP() (*rtp.Packet, interceptor.Attributes, error) {
            b := make([]byte, receiveMTU)
            i, attributes, err := t.Read(b)
            if err != nil {
                return nil, nil, err
            }

            r := &rtp.Packet{}
            if err := r.Unmarshal(b[:i]); err != nil {
                return nil, nil, err
            }
            return r, attributes, nil
        }
    */
    /// determine_payload_type blocks and reads a single packet to determine the PayloadType for this Track
    /// this is useful because we can't announce it to the user until we know the payload_type
    pub(crate) async fn determine_payload_type(&mut self) -> Result<()> {
        let mut b = vec![0u8; RECEIVE_MTU];
        let (n, _) = self.peek(&mut b).await?;

        let mut buf = &b[..n];
        let r = rtp::packet::Packet::unmarshal(&mut buf)?;
        self.payload_type = r.header.payload_type;

        Ok(())
    }

    /// peek is like Read, but it doesn't discard the packet read
    pub(crate) async fn peek(&mut self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        let (n, a) = self.read(b).await?;

        // this might overwrite data if somebody peeked between the Read
        // and us getting the lock.  Oh well, we'll just drop a packet in
        // that case.
        let mut data = BytesMut::new();
        data.extend(b[..n].to_vec());
        self.peeked = Some(data.freeze());
        self.peeked_attributes = Some(a.clone());
        Ok((n, a))
    }
}
