#[cfg(test)]
mod rtp_transceiver_test;

use crate::api::media_engine::MediaEngine;
use crate::error::Error;
use crate::media::rtp::rtp_codec::*;
use crate::media::rtp::rtp_receiver::{RTPReceiver, RTPReceiverInternal};
use crate::media::rtp::rtp_sender::RTCRtpSender;
use crate::media::rtp::rtp_transceiver_direction::RTPTransceiverDirection;
use crate::media::rtp::PayloadType;
use crate::media::track::track_local::TrackLocal;

use anyhow::Result;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use util::Unmarshal;

/// RTPTransceiver represents a combination of an RTPSender and an RTPReceiver that share a common mid.
pub struct RTPTransceiver {
    mid: Mutex<String>,                        //atomic.Value
    sender: Mutex<Option<Arc<RTCRtpSender>>>,  //atomic.Value
    receiver: Mutex<Option<Arc<RTPReceiver>>>, //atomic.Value
    direction: AtomicU8,                       //RTPTransceiverDirection, //atomic.Value

    codecs: Arc<Mutex<Vec<RTPCodecParameters>>>, // User provided codecs via set_codec_preferences

    pub(crate) stopped: bool,
    pub(crate) kind: RTPCodecType,

    media_engine: Arc<MediaEngine>,
}

impl RTPTransceiver {
    pub(crate) async fn new(
        receiver: Option<Arc<RTPReceiver>>,
        sender: Option<Arc<RTCRtpSender>>,
        direction: RTPTransceiverDirection,
        kind: RTPCodecType,
        codecs: Vec<RTPCodecParameters>,
        media_engine: Arc<MediaEngine>,
    ) -> Arc<Self> {
        let t = Arc::new(RTPTransceiver {
            mid: Mutex::new(String::new()),
            sender: Mutex::new(None),
            receiver: Mutex::new(None),
            direction: AtomicU8::new(direction as u8),
            codecs: Arc::new(Mutex::new(codecs)),
            stopped: false,
            kind,
            media_engine,
        });

        t.set_receiver(receiver).await;
        t.set_sender(sender).await;

        t
    }

    /// set_codec_preferences sets preferred list of supported codecs
    /// if codecs is empty or nil we reset to default from MediaEngine
    pub async fn set_codec_preferences(&self, codecs: Vec<RTPCodecParameters>) -> Result<()> {
        for codec in &codecs {
            let media_engine_codecs = self.media_engine.get_codecs_by_kind(self.kind).await;
            let (_, match_type) = codec_parameters_fuzzy_search(codec, &media_engine_codecs);
            if match_type == CodecMatch::None {
                return Err(Error::ErrRTPTransceiverCodecUnsupported.into());
            }
        }

        {
            let mut c = self.codecs.lock().await;
            *c = codecs;
        }
        Ok(())
    }

    /// Codecs returns list of supported codecs
    pub(crate) async fn get_codecs(&self) -> Vec<RTPCodecParameters> {
        let codecs = self.codecs.lock().await;
        RTPReceiverInternal::get_codecs(&*codecs, self.kind, &self.media_engine).await
    }

    /// sender returns the RTPTransceiver's RTPSender if it has one
    pub async fn sender(&self) -> Option<Arc<RTCRtpSender>> {
        let sender = self.sender.lock().await;
        sender.clone()
    }

    /// set_sender_track sets the RTPSender and Track to current transceiver
    pub async fn set_sender_track(
        self: &Arc<Self>,
        sender: Option<Arc<RTCRtpSender>>,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        let _ = self.set_sender(sender).await;
        self.set_sending_track(track).await
    }

    pub async fn set_sender(self: &Arc<Self>, s: Option<Arc<RTCRtpSender>>) {
        if let Some(sender) = &s {
            sender.set_rtp_transceiver(Some(Arc::clone(self))).await;
        }

        if let Some(prev_sender) = self.sender().await {
            prev_sender.set_rtp_transceiver(None).await;
        }

        {
            let mut sender = self.sender.lock().await;
            *sender = s;
        }
    }

    /// receiver returns the RTPTransceiver's RTPReceiver if it has one
    pub async fn receiver(&self) -> Option<Arc<RTPReceiver>> {
        let receiver = self.receiver.lock().await;
        receiver.clone()
    }

    pub(crate) async fn set_receiver(&self, r: Option<Arc<RTPReceiver>>) {
        if let Some(receiver) = &r {
            receiver
                .set_transceiver_codecs(Some(Arc::clone(&self.codecs)))
                .await;
        }

        {
            let mut receiver = self.receiver.lock().await;
            if let Some(prev_receiver) = &*receiver {
                prev_receiver.set_transceiver_codecs(None).await;
            }

            *receiver = r;
        }
    }

    /// set_mid sets the RTPTransceiver's mid. If it was already set, will return an error.
    pub(crate) async fn set_mid(&self, mid: String) -> Result<()> {
        let mut m = self.mid.lock().await;
        if !m.is_empty() {
            return Err(Error::ErrRTPTransceiverCannotChangeMid.into());
        }
        *m = mid;

        Ok(())
    }

    /// mid gets the Transceiver's mid value. When not already set, this value will be set in CreateOffer or create_answer.
    pub async fn mid(&self) -> String {
        let mid = self.mid.lock().await;
        mid.clone()
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
    pub async fn stop(&self) -> Result<()> {
        {
            let s = self.sender.lock().await;
            if let Some(sender) = &*s {
                sender.stop().await?;
            }
        }
        {
            let r = self.receiver.lock().await;
            if let Some(receiver) = &*r {
                receiver.stop().await?;
            }
        }

        self.set_direction(RTPTransceiverDirection::Inactive);

        Ok(())
    }

    pub(crate) async fn set_sending_track(
        &self,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        let track_is_none = track.is_none();
        {
            let mut s = self.sender.lock().await;
            if let Some(sender) = &*s {
                sender.replace_track(track).await?;
            }
            if track_is_none {
                *s = None;
            }
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

pub(crate) async fn find_by_mid(
    mid: &str,
    local_transceivers: &mut Vec<Arc<RTPTransceiver>>,
) -> Option<Arc<RTPTransceiver>> {
    for (i, t) in local_transceivers.iter().enumerate() {
        if t.mid().await == mid {
            return Some(local_transceivers.remove(i));
        }
    }

    None
}

/// Given a direction+type pluck a transceiver from the passed list
/// if no entry satisfies the requested type+direction return a inactive Transceiver
pub(crate) async fn satisfy_type_and_direction(
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
            if t.mid().await.is_empty()
                && t.kind == remote_kind
                && possible_direction == t.direction()
            {
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
