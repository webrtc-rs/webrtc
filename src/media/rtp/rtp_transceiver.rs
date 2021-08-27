use crate::media::rtp::rtp_codec::{
    codec_parameters_fuzzy_search, CodecMatch, RTPCodecParameters, RTPCodecType,
};
use crate::media::rtp::rtp_receiver::RTPReceiver;
use crate::media::rtp::rtp_sender::RTPSender;
use crate::media::rtp::rtp_transceiver_direction::RTPTransceiverDirection;
use crate::media::track::track_local::TrackLocal;

use crate::api::media_engine::MediaEngine;
use crate::error::Error;
use crate::media::rtp::PayloadType;
use anyhow::Result;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use util::Unmarshal;

/// RTPTransceiver represents a combination of an RTPSender and an RTPReceiver that share a common mid.
pub struct RTPTransceiver {
    mid: String,                        //atomic.Value
    sender: Option<Arc<RTPSender>>,     //atomic.Value
    receiver: Option<Arc<RTPReceiver>>, //atomic.Value
    direction: AtomicU8,                //RTPTransceiverDirection, //atomic.Value

    codecs: Vec<RTPCodecParameters>, // User provided codecs via set_codec_preferences

    pub(crate) stopped: bool,
    pub(crate) kind: RTPCodecType,

    media_engine: Arc<MediaEngine>,
}

impl RTPTransceiver {
    pub(crate) fn new(
        receiver: Option<Arc<RTPReceiver>>,
        sender: Option<Arc<RTPSender>>,
        direction: RTPTransceiverDirection,
        kind: RTPCodecType,
        codecs: Vec<RTPCodecParameters>,
        media_engine: Arc<MediaEngine>,
    ) -> Self {
        RTPTransceiver {
            mid: String::new(),
            sender,
            receiver,
            direction: AtomicU8::new(direction as u8),
            codecs,
            stopped: false,
            kind,
            media_engine,
        }
    }

    /// set_codec_preferences sets preferred list of supported codecs
    /// if codecs is empty or nil we reset to default from MediaEngine
    pub async fn set_codec_preferences(&mut self, codecs: Vec<RTPCodecParameters>) -> Result<()> {
        for codec in &codecs {
            let media_engine_codecs = self.media_engine.get_codecs_by_kind(self.kind).await;
            let (_, match_type) = codec_parameters_fuzzy_search(codec, &media_engine_codecs);
            if match_type == CodecMatch::None {
                return Err(Error::ErrRTPTransceiverCodecUnsupported.into());
            }
        }

        self.codecs = codecs;
        Ok(())
    }

    /// Codecs returns list of supported codecs
    pub(crate) async fn get_codecs(&self) -> Vec<RTPCodecParameters> {
        let media_engine_codecs = self.media_engine.get_codecs_by_kind(self.kind).await;
        if self.codecs.is_empty() {
            return media_engine_codecs;
        }

        let mut filtered_codecs = vec![];
        for codec in &self.codecs {
            let (c, match_type) = codec_parameters_fuzzy_search(codec, &media_engine_codecs);
            if match_type != CodecMatch::None {
                filtered_codecs.push(c);
            }
        }

        filtered_codecs
    }

    /// sender returns the RTPTransceiver's RTPSender if it has one
    pub fn sender(&self) -> Option<&Arc<RTPSender>> {
        self.sender.as_ref()
    }

    /// set_sender sets the RTPSender and Track to current transceiver
    pub async fn set_sender(
        &mut self,
        sender: Option<Arc<RTPSender>>,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        self.sender = sender;
        self.set_sending_track(track).await
    }

    /// receiver returns the RTPTransceiver's RTPReceiver if it has one
    pub fn receiver(&self) -> Option<&Arc<RTPReceiver>> {
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

    /// mid gets the Transceiver's mid value. When not already set, this value will be set in CreateOffer or create_answer.
    pub fn mid(&self) -> &str {
        self.mid.as_str()
    }

    /// kind returns RTPTransceiver's kind.
    pub fn kind(&self) -> RTPCodecType {
        self.kind
    }

    /// direction returns the RTPTransceiver's current direction
    pub fn direction(&self) -> RTPTransceiverDirection {
        self.direction.load(Ordering::SeqCst).into()
    }

    pub(crate) fn set_direction(&self, d: RTPTransceiverDirection) {
        self.direction.store(d as u8, Ordering::SeqCst);
    }

    /// stop irreversibly stops the RTPTransceiver
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(_sender) = &mut self.sender {
            //TODO: sender.stop().await?;
        }
        if let Some(_receiver) = &mut self.receiver {
            //TODO: receiver.stop().await?;
        }

        self.set_direction(RTPTransceiverDirection::Inactive);

        Ok(())
    }

    pub(crate) async fn set_sending_track(
        &mut self,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        let track_is_none = track.is_none();
        if let Some(_sender) = &mut self.sender {
            //TODO: sender.replace_track(track).await?;
        }
        if track_is_none {
            self.sender = None;
        }

        let direction = self.direction();
        if !track_is_none && direction == RTPTransceiverDirection::Recvonly {
            self.set_direction(RTPTransceiverDirection::Sendrecv);
        } else if !track_is_none && direction == RTPTransceiverDirection::Inactive {
            self.set_direction(RTPTransceiverDirection::Sendonly);
        } else if track_is_none && direction == RTPTransceiverDirection::Sendrecv {
            self.set_direction(RTPTransceiverDirection::Recvonly);
        } else if !track_is_none
            && (direction == RTPTransceiverDirection::Sendonly
                || direction == RTPTransceiverDirection::Sendrecv)
        {
            // Handle the case where a sendonly transceiver was added by a negotiation
            // initiated by remote peer. For example a remote peer added a transceiver
            // with direction recvonly.
            //} else if !track_is_none && self.direction == RTPTransceiverDirection::Sendrecv {
            // Similar to above, but for sendrecv transceiver.
        } else if track_is_none && direction == RTPTransceiverDirection::Sendonly {
            self.set_direction(RTPTransceiverDirection::Inactive);
        } else {
            return Err(Error::ErrRTPTransceiverSetSendingInvalidState.into());
        }
        Ok(())
    }
}

pub(crate) fn find_by_mid(
    mid: &str,
    local_transceivers: &mut Vec<Arc<RTPTransceiver>>,
) -> Option<Arc<RTPTransceiver>> {
    for (i, t) in local_transceivers.iter().enumerate() {
        if t.mid() == mid {
            return Some(local_transceivers.remove(i));
        }
    }

    None
}

/// Given a direction+type pluck a transceiver from the passed list
/// if no entry satisfies the requested type+direction return a inactive Transceiver
pub(crate) fn satisfy_type_and_direction(
    remote_kind: RTPCodecType,
    remote_direction: RTPTransceiverDirection,
    local_transceivers: &mut Vec<Arc<RTPTransceiver>>,
) -> Option<Arc<RTPTransceiver>> {
    // Get direction order from most preferred to least
    let get_preferred_directions = || -> Vec<RTPTransceiverDirection> {
        match remote_direction {
            RTPTransceiverDirection::Sendrecv => vec![
                RTPTransceiverDirection::Recvonly,
                RTPTransceiverDirection::Sendrecv,
            ],
            RTPTransceiverDirection::Sendonly => vec![RTPTransceiverDirection::Recvonly],
            RTPTransceiverDirection::Recvonly => vec![
                RTPTransceiverDirection::Sendonly,
                RTPTransceiverDirection::Sendrecv,
            ],
            _ => vec![],
        }
    };

    for possible_direction in get_preferred_directions() {
        for (i, t) in local_transceivers.iter().enumerate() {
            if t.mid() == "" && t.kind == remote_kind && possible_direction == t.direction() {
                return Some(local_transceivers.remove(i));
            }
        }
    }

    None
}

/// handle_unknown_rtp_packet consumes a single RTP Packet and returns information that is helpful
/// for demuxing and handling an unknown SSRC (usually for Simulcast)
pub(crate) fn handle_unknown_rtp_packet(
    buf: &[u8],
    mid_extension_id: u8,
    sid_extension_id: u8,
) -> Result<(String, String, PayloadType)> {
    let mut reader = buf;
    let rp = rtp::packet::Packet::unmarshal(&mut reader)?;

    if !rp.header.extension {
        return Ok((String::new(), String::new(), 0));
    }

    let payload_type = rp.header.payload_type;

    let mid = if let Some(payload) = rp.header.get_extension(mid_extension_id) {
        String::from_utf8(payload.to_vec())?
    } else {
        String::new()
    };

    let rid = if let Some(payload) = rp.header.get_extension(sid_extension_id) {
        String::from_utf8(payload.to_vec())?
    } else {
        String::new()
    };

    Ok((mid, rid, payload_type))
}
