use crate::media::interceptor::Attributes;
use crate::media::rtp::rtp_codec::{RTPCodecParameters, RTPCodecType, RTPParameters};
use crate::media::rtp::rtp_receiver::RTPReceiver;
use crate::media::rtp::{PayloadType, SSRC};

use bytes::Bytes;

/// TrackRemote represents a single inbound source of media
#[derive(Default, Debug, Clone)]
pub struct TrackRemote {
    id: String,
    stream_id: String,

    payload_type: PayloadType,
    kind: RTPCodecType,
    ssrc: SSRC,
    codec: RTPCodecParameters,
    params: RTPParameters,
    rid: String,

    receiver: RTPReceiver,
    peeked: Bytes,
    peeked_attributes: Attributes,
}

impl TrackRemote {
    pub(crate) fn new(kind: RTPCodecType, ssrc: SSRC, rid: String, receiver: RTPReceiver) -> Self {
        TrackRemote {
            kind,
            ssrc,
            rid,
            receiver,
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
    /*
    // Read reads data from the track.
    func (t *TrackRemote) Read(b []byte) (n int, attributes interceptor.Attributes, err error) {
        t.mu.RLock()
        r := t.receiver
        peeked := t.peeked != nil
        t.mu.RUnlock()

        if peeked {
            t.mu.Lock()
            data := t.peeked
            attributes = t.peeked_attributes

            t.peeked = nil
            t.peeked_attributes = nil
            t.mu.Unlock()
            // someone else may have stolen our packet when we
            // released the lock.  Deal with it.
            if data != nil {
                n = copy(b, data)
                return
            }
        }

        return r.readRTP(b, t)
    }

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

    // determinePayloadType blocks and reads a single packet to determine the PayloadType for this Track
    // this is useful because we can't announce it to the user until we know the payload_type
    func (t *TrackRemote) determinePayloadType() error {
        b := make([]byte, receiveMTU)
        n, _, err := t.peek(b)
        if err != nil {
            return err
        }
        r := rtp.Packet{}
        if err := r.Unmarshal(b[:n]); err != nil {
            return err
        }

        t.mu.Lock()
        t.payload_type = PayloadType(r.PayloadType)
        defer t.mu.Unlock()

        return nil
    }

    // peek is like Read, but it doesn't discard the packet read
    func (t *TrackRemote) peek(b []byte) (n int, a interceptor.Attributes, err error) {
        n, a, err = t.Read(b)
        if err != nil {
            return
        }

        t.mu.Lock()
        // this might overwrite data if somebody peeked between the Read
        // and us getting the lock.  Oh well, we'll just drop a packet in
        // that case.
        data := make([]byte, n)
        n = copy(data, b[:n])
        t.peeked = data
        t.peeked_attributes = a
        t.mu.Unlock()
        return
    }

    // SetReadDeadline sets the max amount of time the RTP stream will block before returning. 0 is forever.
    func (t *TrackRemote) SetReadDeadline(deadline time.Time) error {
        return t.receiver.setRTPReadDeadline(deadline, t)
    }
    */
}
