use crate::media::rtp::rtp_codec::{
    codec_parameters_fuzzy_search, CodecMatchType, RTPCodecParameters, RTPCodecType,
};
use crate::media::rtp::rtp_receiver::RTPReceiver;
use crate::media::rtp::rtp_sender::RTPSender;
use crate::media::rtp::rtp_transceiver_direction::RTPTransceiverDirection;
use crate::media::track::track_local::TrackLocal;

use crate::api::media_engine::MediaEngine;
use crate::error::Error;
use anyhow::Result;
use std::sync::Arc;

/// RTPTransceiver represents a combination of an RTPSender and an RTPReceiver that share a common mid.
pub struct RTPTransceiver {
    mid: String,                        //atomic.Value
    sender: Option<RTPSender>,          //atomic.Value
    receiver: Option<RTPReceiver>,      //atomic.Value
    direction: RTPTransceiverDirection, //atomic.Value

    codecs: Vec<RTPCodecParameters>, // User provided codecs via set_codec_preferences

    stopped: bool,
    pub(crate) kind: RTPCodecType,

    media_engine: Arc<MediaEngine>,
}

impl RTPTransceiver {
    pub(crate) fn new(
        receiver: Option<RTPReceiver>,
        sender: Option<RTPSender>,
        direction: RTPTransceiverDirection,
        kind: RTPCodecType,
        media_engine: Arc<MediaEngine>,
    ) -> Self {
        RTPTransceiver {
            mid: String::new(),
            sender,
            receiver,
            direction,
            codecs: vec![],
            stopped: false,
            kind,
            media_engine,
        }
    }

    /// set_codec_preferences sets preferred list of supported codecs
    /// if codecs is empty or nil we reset to default from MediaEngine
    pub fn set_codec_preferences(&mut self, codecs: Vec<RTPCodecParameters>) -> Result<()> {
        for codec in &codecs {
            let media_engine_codecs = self.media_engine.get_codecs_by_kind(self.kind);
            let (_, match_type) = codec_parameters_fuzzy_search(codec, &media_engine_codecs);
            if match_type == CodecMatchType::None {
                return Err(Error::ErrRTPTransceiverCodecUnsupported.into());
            }
        }

        self.codecs = codecs;
        Ok(())
    }

    /// Codecs returns list of supported codecs
    pub(crate) fn get_codecs(&self) -> Vec<RTPCodecParameters> {
        let media_engine_codecs = self.media_engine.get_codecs_by_kind(self.kind);
        if self.codecs.is_empty() {
            return media_engine_codecs;
        }

        let mut filtered_codecs = vec![];
        for codec in &self.codecs {
            let (c, match_type) = codec_parameters_fuzzy_search(codec, &media_engine_codecs);
            if match_type != CodecMatchType::None {
                filtered_codecs.push(c);
            }
        }

        filtered_codecs
    }

    /// sender returns the RTPTransceiver's RTPSender if it has one
    pub fn sender(&self) -> Option<&RTPSender> {
        self.sender.as_ref()
    }

    /// set_sender sets the RTPSender and Track to current transceiver
    pub async fn set_sender(
        &mut self,
        sender: Option<RTPSender>,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        self.sender = sender;
        self.set_sending_track(track).await
    }

    /// receiver returns the RTPTransceiver's RTPReceiver if it has one
    pub fn receiver(&self) -> Option<&RTPReceiver> {
        self.receiver.as_ref()
    }

    /// set_mid sets the RTPTransceiver's mid. If it was already set, will return an error.
    pub(crate) fn set_mid(&mut self, mid: String) -> Result<()> {
        if !self.mid.is_empty() {
            return Err(Error::ErrRTPTransceiverCannotChangeMid.into());
        }
        self.mid = mid;

        Ok(())
    }

    /// mid gets the Transceiver's mid value. When not already set, this value will be set in CreateOffer or CreateAnswer.
    pub fn mid(&self) -> String {
        self.mid.clone()
    }

    /// kind returns RTPTransceiver's kind.
    pub fn kind(&self) -> RTPCodecType {
        self.kind
    }

    /// direction returns the RTPTransceiver's current direction
    pub fn direction(&self) -> RTPTransceiverDirection {
        self.direction
    }

    /// stop irreversibly stops the RTPTransceiver
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(sender) = &mut self.sender {
            sender.stop().await?;
        }
        if let Some(receiver) = &mut self.receiver {
            receiver.stop().await?;
        }

        self.direction = RTPTransceiverDirection::Inactive;

        Ok(())
    }

    pub(crate) async fn set_sending_track(
        &mut self,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        let track_is_none = track.is_none();
        if let Some(sender) = &mut self.sender {
            sender.replace_track(track).await?;
        }
        if track_is_none {
            self.sender = None;
        }

        if !track_is_none && self.direction == RTPTransceiverDirection::Recvonly {
            self.direction = RTPTransceiverDirection::Sendrecv;
        } else if !track_is_none && self.direction == RTPTransceiverDirection::Inactive {
            self.direction = RTPTransceiverDirection::Sendonly;
        } else if track_is_none && self.direction == RTPTransceiverDirection::Sendrecv {
            self.direction = RTPTransceiverDirection::Recvonly;
        } else if !track_is_none
            && (self.direction == RTPTransceiverDirection::Sendonly
                || self.direction == RTPTransceiverDirection::Sendrecv)
        {
            // Handle the case where a sendonly transceiver was added by a negotiation
            // initiated by remote peer. For example a remote peer added a transceiver
            // with direction recvonly.
            //} else if !track_is_none && self.direction == RTPTransceiverDirection::Sendrecv {
            // Similar to above, but for sendrecv transceiver.
        } else if track_is_none && self.direction == RTPTransceiverDirection::Sendonly {
            self.direction = RTPTransceiverDirection::Inactive;
        } else {
            return Err(Error::ErrRTPTransceiverSetSendingInvalidState.into());
        }
        Ok(())
    }
}

/*TODO:
pub(crate) fn find_by_mid(mid:&str, localTransceivers: Vec[RTPTransceiver]) ->(RTPTransceiver, []*RTPTransceiver) {
    for i, t := range localTransceivers {
        if t.Mid() == mid {
            return t, append(localTransceivers[:i], localTransceivers[i+1:]...)
        }
    }

    return nil, localTransceivers
}


// Given a direction+type pluck a transceiver from the passed list
// if no entry satisfies the requested type+direction return a inactive Transceiver
func satisfyTypeAndDirection(remoteKind RTPCodecType, remoteDirection RTPTransceiverDirection, localTransceivers []*RTPTransceiver) (*RTPTransceiver, []*RTPTransceiver) {
    // Get direction order from most preferred to least
    getPreferredDirections := func() []RTPTransceiverDirection {
        switch remoteDirection {
        case RTPTransceiverDirectionSendrecv:
            return []RTPTransceiverDirection{RTPTransceiverDirectionRecvonly, RTPTransceiverDirectionSendrecv}
        case RTPTransceiverDirectionSendonly:
            return []RTPTransceiverDirection{RTPTransceiverDirectionRecvonly}
        case RTPTransceiverDirectionRecvonly:
            return []RTPTransceiverDirection{RTPTransceiverDirectionSendonly, RTPTransceiverDirectionSendrecv}
        default:
            return []RTPTransceiverDirection{}
        }
    }

    for _, possibleDirection := range getPreferredDirections() {
        for i := range localTransceivers {
            t := localTransceivers[i]
            if t.Mid() == "" && t.kind == remoteKind && possibleDirection == t.Direction() {
                return t, append(localTransceivers[:i], localTransceivers[i+1:]...)
            }
        }
    }

    return nil, localTransceivers
}

// handleUnknownRTPPacket consumes a single RTP Packet and returns information that is helpful
// for demuxing and handling an unknown SSRC (usually for Simulcast)
func handleUnknownRTPPacket(buf []byte, midExtensionID, streamIDExtensionID uint8) (mid, rid string, payloadType PayloadType, err error) {
    rp := &rtp.Packet{}
    if err = rp.Unmarshal(buf); err != nil {
        return
    }

    if !rp.Header.Extension {
        return
    }

    payloadType = PayloadType(rp.PayloadType)
    if payload := rp.GetExtension(midExtensionID); payload != nil {
        mid = string(payload)
    }

    if payload := rp.GetExtension(streamIDExtensionID); payload != nil {
        rid = string(payload)
    }

    return
}
*/
