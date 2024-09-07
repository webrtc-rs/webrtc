#[cfg(test)]
mod sdp_test;

use crate::api::media_engine::MediaEngine;
use crate::dtls_transport::dtls_fingerprint::RTCDtlsFingerprint;
use crate::error::{Error, Result};
use crate::ice_transport::ice_candidate::RTCIceCandidate;
use crate::ice_transport::ice_gatherer::RTCIceGatherer;
use crate::ice_transport::ice_gathering_state::RTCIceGatheringState;
use crate::ice_transport::ice_parameters::RTCIceParameters;
use crate::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType,
};
use crate::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use crate::rtp_transceiver::{PayloadType, RTCPFeedback, RTCRtpTransceiver, SSRC};

pub mod sdp_type;
pub mod session_description;

use std::collections::HashMap;
use std::convert::From;
use std::io::BufReader;
use std::sync::Arc;

use ice::candidate::candidate_base::unmarshal_candidate;
use ice::candidate::Candidate;
use sdp::description::common::{Address, ConnectionInformation};
use sdp::description::media::{MediaDescription, MediaName, RangedPort};
use sdp::description::session::*;
use sdp::extmap::ExtMap;
use sdp::util::ConnectionRole;
use smol_str::SmolStr;
use url::Url;

use crate::peer_connection::MEDIA_SECTION_APPLICATION;
use crate::{SDP_ATTRIBUTE_RID, SDP_ATTRIBUTE_SIMULCAST};

/// TrackDetails represents any media source that can be represented in a SDP
/// This isn't keyed by SSRC because it also needs to support rid based sources
#[derive(Default, Debug, Clone)]
pub(crate) struct TrackDetails {
    pub(crate) mid: SmolStr,
    pub(crate) kind: RTPCodecType,
    pub(crate) stream_id: String,
    pub(crate) id: String,
    pub(crate) ssrcs: Vec<SSRC>,
    pub(crate) repair_ssrc: SSRC,
    pub(crate) rids: Vec<SmolStr>,
}

pub(crate) fn track_details_for_ssrc(
    track_details: &[TrackDetails],
    ssrc: SSRC,
) -> Option<&TrackDetails> {
    track_details.iter().find(|x| x.ssrcs.contains(&ssrc))
}

pub(crate) fn track_details_for_rid(
    track_details: &[TrackDetails],
    rid: SmolStr,
) -> Option<&TrackDetails> {
    track_details.iter().find(|x| x.rids.contains(&rid))
}

pub(crate) fn filter_track_with_ssrc(incoming_tracks: &mut Vec<TrackDetails>, ssrc: SSRC) {
    incoming_tracks.retain(|x| !x.ssrcs.contains(&ssrc));
}

/// extract all TrackDetails from an SDP.
pub(crate) fn track_details_from_sdp(
    s: &SessionDescription,
    exclude_inactive: bool,
) -> Vec<TrackDetails> {
    let mut incoming_tracks = vec![];

    for media in &s.media_descriptions {
        let mut tracks_in_media_section = vec![];
        let mut rtx_repair_flows = HashMap::new();

        let mut stream_id = "";
        let mut track_id = "";

        // If media section is recvonly or inactive skip
        if media.attribute(ATTR_KEY_RECV_ONLY).is_some()
            || (exclude_inactive && media.attribute(ATTR_KEY_INACTIVE).is_some())
        {
            continue;
        }

        let mid_value = match get_mid_value(media) {
            Some(mid_value) => mid_value,
            None => continue,
        };

        let codec_type = RTPCodecType::from(media.media_name.media.as_str());
        if codec_type == RTPCodecType::Unspecified {
            continue;
        }

        for attr in &media.attributes {
            match attr.key.as_str() {
                ATTR_KEY_SSRCGROUP => {
                    if let Some(value) = &attr.value {
                        let split: Vec<&str> = value.split(' ').collect();
                        if split[0] == SEMANTIC_TOKEN_FLOW_IDENTIFICATION {
                            // Add rtx ssrcs to blacklist, to avoid adding them as tracks
                            // Essentially lines like `a=ssrc-group:FID 2231627014 632943048` are processed by this section
                            // as this declares that the second SSRC (632943048) is a rtx repair flow (RFC4588) for the first
                            // (2231627014) as specified in RFC5576
                            if split.len() == 3 {
                                let base_ssrc = match split[1].parse::<u32>() {
                                    Ok(ssrc) => ssrc,
                                    Err(err) => {
                                        log::warn!("Failed to parse SSRC: {}", err);
                                        continue;
                                    }
                                };
                                let rtx_repair_flow = match split[2].parse::<u32>() {
                                    Ok(n) => n,
                                    Err(err) => {
                                        log::warn!("Failed to parse SSRC: {}", err);
                                        continue;
                                    }
                                };
                                rtx_repair_flows.insert(rtx_repair_flow, base_ssrc);
                                // Remove if rtx was added as track before
                                filter_track_with_ssrc(
                                    &mut tracks_in_media_section,
                                    rtx_repair_flow as SSRC,
                                );
                            }
                        }
                    }
                }

                // Handle `a=msid:<stream_id> <track_label>` The first value is the same as MediaStream.id
                // in the browser and can be used to figure out which tracks belong to the same stream. The browser should
                // figure this out automatically when an ontrack event is emitted on RTCPeerConnection.
                ATTR_KEY_MSID => {
                    if let Some(value) = &attr.value {
                        let mut split = value.split(' ');

                        if let (Some(sid), Some(tid), None) =
                            (split.next(), split.next(), split.next())
                        {
                            stream_id = sid;
                            track_id = tid;
                        }
                    }
                }

                ATTR_KEY_SSRC => {
                    if let Some(value) = &attr.value {
                        let split: Vec<&str> = value.split(' ').collect();
                        let ssrc = match split[0].parse::<u32>() {
                            Ok(ssrc) => ssrc,
                            Err(err) => {
                                log::warn!("Failed to parse SSRC: {}", err);
                                continue;
                            }
                        };

                        if rtx_repair_flows.contains_key(&ssrc) {
                            continue; // This ssrc is a RTX repair flow, ignore
                        }

                        if split.len() == 3 && split[1].starts_with("msid:") {
                            stream_id = &split[1]["msid:".len()..];
                            track_id = split[2];
                        }

                        let mut track_idx = tracks_in_media_section.len();

                        for (i, t) in tracks_in_media_section.iter().enumerate() {
                            if t.ssrcs.contains(&ssrc) {
                                track_idx = i;
                                //TODO: no break?
                            }
                        }

                        if track_idx < tracks_in_media_section.len() {
                            tracks_in_media_section[track_idx].mid = SmolStr::from(mid_value);
                            tracks_in_media_section[track_idx].kind = codec_type;
                            stream_id.clone_into(&mut tracks_in_media_section[track_idx].stream_id);
                            track_id.clone_into(&mut tracks_in_media_section[track_idx].id);
                            tracks_in_media_section[track_idx].ssrcs = vec![ssrc];
                        } else {
                            let track_details = TrackDetails {
                                mid: SmolStr::from(mid_value),
                                kind: codec_type,
                                stream_id: stream_id.to_owned(),
                                id: track_id.to_owned(),
                                ssrcs: vec![ssrc],
                                ..Default::default()
                            };
                            tracks_in_media_section.push(track_details);
                        }
                    }
                }
                _ => {}
            };
        }
        for (repair, base) in &rtx_repair_flows {
            for track in &mut tracks_in_media_section {
                if track.ssrcs.contains(base) {
                    track.repair_ssrc = *repair;
                }
            }
        }

        // If media line is using RTP Stream Identifier Source Description per RFC8851
        // we will need to override tracks, and remove ssrcs.
        // This is in particular important for Firefox, as it uses both 'rid', 'simulcast'
        // and 'a=ssrc' lines.
        let rids = get_rids(media);
        if !rids.is_empty() && !track_id.is_empty() && !stream_id.is_empty() {
            tracks_in_media_section = vec![TrackDetails {
                mid: SmolStr::from(mid_value),
                kind: codec_type,
                stream_id: stream_id.to_owned(),
                id: track_id.to_owned(),
                rids: rids.iter().map(|r| SmolStr::from(&r.id)).collect(),
                ..Default::default()
            }];
        }

        incoming_tracks.extend(tracks_in_media_section);
    }

    incoming_tracks
}

pub(crate) fn get_rids(media: &MediaDescription) -> Vec<SimulcastRid> {
    let mut rids = vec![];
    let mut simulcast_attr: Option<String> = None;
    for attr in &media.attributes {
        if attr.key.as_str() == SDP_ATTRIBUTE_RID {
            if let Err(err) = attr
                .value
                .as_ref()
                .ok_or(SimulcastRidParseError::SyntaxIdDirSplit)
                .and_then(SimulcastRid::try_from)
                .map(|rid| rids.push(rid))
            {
                log::warn!("Failed to parse RID: {}", err);
            }
        } else if attr.key.as_str() == SDP_ATTRIBUTE_SIMULCAST {
            simulcast_attr.clone_from(&attr.value);
        }
    }

    if let Some(attr) = simulcast_attr {
        let mut split = attr.split(' ');
        loop {
            let _dir = split.next();
            let sc_str_list = split.next();
            if let Some(list) = sc_str_list {
                let sc_list: Vec<&str> = list.split(';').flat_map(|alt| alt.split(',')).collect();
                for sc_id in sc_list {
                    let (sc_id, paused) = if let Some(sc_id) = sc_id.strip_prefix('~') {
                        (sc_id, true)
                    } else {
                        (sc_id, false)
                    };

                    if let Some(rid) = rids.iter_mut().find(|f| f.id == sc_id) {
                        rid.paused = paused;
                    }
                }
            } else {
                break;
            }
        }
    }

    rids
}

pub(crate) async fn add_candidates_to_media_descriptions(
    candidates: &[RTCIceCandidate],
    mut m: MediaDescription,
    ice_gathering_state: RTCIceGatheringState,
) -> Result<MediaDescription> {
    let append_candidate_if_new = |c: &dyn Candidate, m: MediaDescription| -> MediaDescription {
        let marshaled = c.marshal();
        for a in &m.attributes {
            if let Some(value) = &a.value {
                if &marshaled == value {
                    return m;
                }
            }
        }

        m.with_value_attribute("candidate".to_owned(), marshaled)
    };

    for c in candidates {
        let candidate = c.to_ice()?;

        candidate.set_component(1);
        m = append_candidate_if_new(&candidate, m);

        candidate.set_component(2);
        m = append_candidate_if_new(&candidate, m);
    }

    if ice_gathering_state != RTCIceGatheringState::Complete {
        return Ok(m);
    }
    for a in &m.attributes {
        if &a.key == "end-of-candidates" {
            return Ok(m);
        }
    }

    Ok(m.with_property_attribute("end-of-candidates".to_owned()))
}

pub(crate) struct AddDataMediaSectionParams {
    should_add_candidates: bool,
    mid_value: String,
    ice_params: RTCIceParameters,
    dtls_role: ConnectionRole,
    ice_gathering_state: RTCIceGatheringState,
}

pub(crate) async fn add_data_media_section(
    d: SessionDescription,
    dtls_fingerprints: &[RTCDtlsFingerprint],
    candidates: &[RTCIceCandidate],
    params: AddDataMediaSectionParams,
) -> Result<SessionDescription> {
    let mut media = MediaDescription {
        media_name: MediaName {
            media: MEDIA_SECTION_APPLICATION.to_owned(),
            port: RangedPort {
                value: 9,
                range: None,
            },
            protos: vec!["UDP".to_owned(), "DTLS".to_owned(), "SCTP".to_owned()],
            formats: vec!["webrtc-datachannel".to_owned()],
        },
        media_title: None,
        connection_information: Some(ConnectionInformation {
            network_type: "IN".to_owned(),
            address_type: "IP4".to_owned(),
            address: Some(Address {
                address: "0.0.0.0".to_owned(),
                ttl: None,
                range: None,
            }),
        }),
        bandwidth: vec![],
        encryption_key: None,
        attributes: vec![],
    }
    .with_value_attribute(
        ATTR_KEY_CONNECTION_SETUP.to_owned(),
        params.dtls_role.to_string(),
    )
    .with_value_attribute(ATTR_KEY_MID.to_owned(), params.mid_value)
    .with_property_attribute(RTCRtpTransceiverDirection::Sendrecv.to_string())
    .with_property_attribute("sctp-port:5000".to_owned())
    .with_ice_credentials(
        params.ice_params.username_fragment,
        params.ice_params.password,
    );

    for f in dtls_fingerprints {
        media = media.with_fingerprint(f.algorithm.clone(), f.value.to_uppercase());
    }

    if params.should_add_candidates {
        media = add_candidates_to_media_descriptions(candidates, media, params.ice_gathering_state)
            .await?;
    }

    Ok(d.with_media(media))
}

pub(crate) async fn populate_local_candidates(
    session_description: Option<&session_description::RTCSessionDescription>,
    ice_gatherer: Option<&Arc<RTCIceGatherer>>,
    ice_gathering_state: RTCIceGatheringState,
) -> Option<session_description::RTCSessionDescription> {
    if session_description.is_none() || ice_gatherer.is_none() {
        return session_description.cloned();
    }

    if let (Some(sd), Some(ice)) = (session_description, ice_gatherer) {
        let candidates = match ice.get_local_candidates().await {
            Ok(candidates) => candidates,
            Err(_) => return Some(sd.clone()),
        };

        let mut parsed = match sd.unmarshal() {
            Ok(parsed) => parsed,
            Err(_) => return Some(sd.clone()),
        };

        if !parsed.media_descriptions.is_empty() {
            let mut m = parsed.media_descriptions.remove(0);
            m = match add_candidates_to_media_descriptions(&candidates, m, ice_gathering_state)
                .await
            {
                Ok(m) => m,
                Err(_) => return Some(sd.clone()),
            };
            parsed.media_descriptions.insert(0, m);
        }

        Some(session_description::RTCSessionDescription {
            sdp_type: sd.sdp_type,
            sdp: parsed.marshal(),
            parsed: Some(parsed),
        })
    } else {
        None
    }
}

pub(crate) struct AddTransceiverSdpParams {
    should_add_candidates: bool,
    mid_value: String,
    dtls_role: ConnectionRole,
    ice_gathering_state: RTCIceGatheringState,
    offered_direction: Option<RTCRtpTransceiverDirection>,
}

pub(crate) async fn add_transceiver_sdp(
    mut d: SessionDescription,
    dtls_fingerprints: &[RTCDtlsFingerprint],
    media_engine: &Arc<MediaEngine>,
    ice_params: &RTCIceParameters,
    candidates: &[RTCIceCandidate],
    media_section: &MediaSection,
    params: AddTransceiverSdpParams,
) -> Result<(SessionDescription, bool)> {
    if media_section.transceivers.is_empty() {
        return Err(Error::ErrSDPZeroTransceivers);
    }
    let (should_add_candidates, mid_value, dtls_role, ice_gathering_state) = (
        params.should_add_candidates,
        params.mid_value,
        params.dtls_role,
        params.ice_gathering_state,
    );

    let transceivers = &media_section.transceivers;
    // Use the first transceiver to generate the section attributes
    let t = &transceivers[0];
    let mut media = MediaDescription::new_jsep_media_description(t.kind.to_string(), vec![])
        .with_value_attribute(ATTR_KEY_CONNECTION_SETUP.to_owned(), dtls_role.to_string())
        .with_value_attribute(ATTR_KEY_MID.to_owned(), mid_value.clone())
        .with_ice_credentials(
            ice_params.username_fragment.clone(),
            ice_params.password.clone(),
        )
        .with_property_attribute(ATTR_KEY_RTCPMUX.to_owned())
        .with_property_attribute(ATTR_KEY_RTCPRSIZE.to_owned());

    if media_section.extmap_allow_mixed {
        media = media.with_property_attribute(ATTR_KEY_EXTMAP_ALLOW_MIXED.to_owned());
    }

    let codecs = t.get_codecs().await;
    for codec in &codecs {
        let name = codec
            .capability
            .mime_type
            .trim_start_matches("audio/")
            .trim_start_matches("video/")
            .to_owned();
        media = media.with_codec(
            codec.payload_type,
            name,
            codec.capability.clock_rate,
            codec.capability.channels,
            codec.capability.sdp_fmtp_line.clone(),
        );

        for feedback in &codec.capability.rtcp_feedback {
            media = media.with_value_attribute(
                "rtcp-fb".to_owned(),
                format!(
                    "{} {} {}",
                    codec.payload_type, feedback.typ, feedback.parameter
                ),
            );
        }
    }
    if codecs.is_empty() {
        // If we are sender and we have no codecs throw an error early
        if t.sender().await.track().await.is_some() {
            return Err(Error::ErrSenderWithNoCodecs);
        }

        // Explicitly reject track if we don't have the codec
        d = d.with_media(MediaDescription {
            media_name: sdp::description::media::MediaName {
                media: t.kind.to_string(),
                port: RangedPort {
                    value: 0,
                    range: None,
                },
                protos: vec![
                    "UDP".to_owned(),
                    "TLS".to_owned(),
                    "RTP".to_owned(),
                    "SAVPF".to_owned(),
                ],
                formats: vec!["0".to_owned()],
            },
            media_title: None,
            // We need to include connection information even if we're rejecting a track, otherwise Firefox will fail to
            // parse the SDP with an error like:
            // SIPCC Failed to parse SDP: SDP Parse Error on line 50:  c= connection line not specified for every media level, validation failed.
            // In addition this makes our SDP compliant with RFC 4566 Section 5.7: https://datatracker.ietf.org/doc/html/rfc4566#section-5.7
            connection_information: Some(ConnectionInformation {
                network_type: "IN".to_owned(),
                address_type: "IP4".to_owned(),
                address: Some(Address {
                    address: "0.0.0.0".to_owned(),
                    ttl: None,
                    range: None,
                }),
            }),
            bandwidth: vec![],
            encryption_key: None,
            attributes: vec![],
        });
        return Ok((d, false));
    }

    let parameters = media_engine.get_rtp_parameters_by_kind(t.kind, t.direction());
    for rtp_extension in &parameters.header_extensions {
        let ext_url = Url::parse(rtp_extension.uri.as_str())?;
        media = media.with_extmap(sdp::extmap::ExtMap {
            value: rtp_extension.id,
            uri: Some(ext_url),
            ..Default::default()
        });
    }

    if !media_section.rid_map.is_empty() {
        let mut recv_sc_list: Vec<String> = vec![];
        let mut send_sc_list: Vec<String> = vec![];

        for rid in &media_section.rid_map {
            let rid_syntax = match rid.direction {
                SimulcastDirection::Send => {
                    // If Send rid, then reply with a recv rid
                    if rid.paused {
                        recv_sc_list.push(format!("~{}", rid.id));
                    } else {
                        recv_sc_list.push(rid.id.to_owned());
                    }
                    format!("{} recv", rid.id)
                }
                SimulcastDirection::Recv => {
                    // If Recv rid, then reply with a send rid
                    if rid.paused {
                        send_sc_list.push(format!("~{}", rid.id));
                    } else {
                        send_sc_list.push(rid.id.to_owned());
                    }
                    format!("{} send", rid.id)
                }
            };
            media = media.with_value_attribute(SDP_ATTRIBUTE_RID.to_owned(), rid_syntax);
        }

        // Simulcast
        let mut sc_attr = String::new();
        if !recv_sc_list.is_empty() {
            sc_attr.push_str(&format!("recv {}", recv_sc_list.join(";")));
        }
        if !send_sc_list.is_empty() {
            sc_attr.push_str(&format!("send {}", send_sc_list.join(";")));
        }
        media = media.with_value_attribute(SDP_ATTRIBUTE_SIMULCAST.to_owned(), sc_attr);
    }

    for mt in transceivers {
        let sender = mt.sender().await;
        if let Some(track) = sender.track().await {
            let send_parameters = sender.get_parameters().await;
            for encoding in &send_parameters.encodings {
                media = media.with_media_source(
                    encoding.ssrc,
                    track.stream_id().to_owned(), /* cname */
                    track.stream_id().to_owned(), /* streamLabel */
                    track.id().to_owned(),
                );

                if encoding.rtx.ssrc != 0 {
                    media = media.with_media_source(
                        encoding.rtx.ssrc,
                        track.stream_id().to_owned(),
                        track.stream_id().to_owned(),
                        track.id().to_owned(),
                    );

                    media = media.with_value_attribute(
                        ATTR_KEY_SSRCGROUP.to_owned(),
                        format!(
                            "{} {} {}",
                            SEMANTIC_TOKEN_FLOW_IDENTIFICATION, encoding.ssrc, encoding.rtx.ssrc
                        ),
                    );
                }
            }

            if send_parameters.encodings.len() > 1 {
                let mut send_rids = Vec::with_capacity(send_parameters.encodings.len());

                for encoding in &send_parameters.encodings {
                    media = media.with_value_attribute(
                        SDP_ATTRIBUTE_RID.to_owned(),
                        format!("{} send", encoding.rid),
                    );
                    send_rids.push(encoding.rid.to_string());
                }

                media = media.with_value_attribute(
                    SDP_ATTRIBUTE_SIMULCAST.to_owned(),
                    format!("send {}", send_rids.join(";")),
                );
            }

            // Send msid based on the configured track if we haven't already
            // sent on this sender. If we have sent we must keep the msid line consistent, this
            // is handled below.
            if sender.initial_track_id().is_none() {
                for stream_id in sender.associated_media_stream_ids() {
                    media =
                        media.with_property_attribute(format!("msid:{} {}", stream_id, track.id()));
                }

                sender.set_initial_track_id(track.id().to_string())?;
                break;
            }
        }

        if let Some(track_id) = sender.initial_track_id() {
            // After we have include an msid attribute in an offer it must stay the same for
            // all subsequent offer even if the track or transceiver direction changes.
            //
            // [RFC 8829 Section 5.2.2](https://datatracker.ietf.org/doc/html/rfc8829#section-5.2.2)
            //
            // For RtpTransceivers that are not stopped, the "a=msid" line or
            // lines MUST stay the same if they are present in the current
            // description, regardless of changes to the transceiver's direction
            // or track.  If no "a=msid" line is present in the current
            // description, "a=msid" line(s) MUST be generated according to the
            // same rules as for an initial offer.
            for stream_id in sender.associated_media_stream_ids() {
                media = media.with_property_attribute(format!("msid:{stream_id} {track_id}"));
            }

            break;
        }
    }

    let direction = match params.offered_direction {
        Some(offered_direction) => {
            use RTCRtpTransceiverDirection::*;
            let transceiver_direction = t.direction();

            match offered_direction {
                Sendonly | Recvonly => {
                    // If a stream is offered as sendonly, the corresponding stream MUST be
                    // marked as recvonly or inactive in the answer.

                    // If a media stream is
                    // listed as recvonly in the offer, the answer MUST be marked as
                    // sendonly or inactive in the answer.
                    offered_direction.reverse().intersect(transceiver_direction)
                }
                // If an offered media stream is
                // listed as sendrecv (or if there is no direction attribute at the
                // media or session level, in which case the stream is sendrecv by
                // default), the corresponding stream in the answer MAY be marked as
                // sendonly, recvonly, sendrecv, or inactive
                Sendrecv | Unspecified => t.direction(),
                // If an offered media
                // stream is listed as inactive, it MUST be marked as inactive in the
                // answer.
                Inactive => Inactive,
            }
        }
        None => {
            // If don't have an offered direction to intersect with just use the transceivers
            // current direction.
            //
            // https://datatracker.ietf.org/doc/html/rfc8829#section-4.2.3
            //
            //    When creating offers, the transceiver direction is directly reflected
            //    in the output, even for re-offers.
            t.direction()
        }
    };
    media = media.with_property_attribute(direction.to_string());

    for fingerprint in dtls_fingerprints {
        media = media.with_fingerprint(
            fingerprint.algorithm.to_owned(),
            fingerprint.value.to_uppercase(),
        );
    }

    if should_add_candidates {
        media =
            add_candidates_to_media_descriptions(candidates, media, ice_gathering_state).await?;
    }

    Ok((d.with_media(media), true))
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub(crate) enum SimulcastRidParseError {
    /// SyntaxIdDirSplit indicates rid-syntax could not be parsed.
    #[error("RFC8851 mandates rid-syntax        = %s\"a=rid:\" rid-id SP rid-dir")]
    SyntaxIdDirSplit,
    /// UnknownDirection indicates rid-dir was not parsed. Should be "send" or "recv".
    #[error("RFC8851 mandates rid-dir           = %s\"send\" / %s\"recv\"")]
    UnknownDirection,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum SimulcastDirection {
    Send,
    Recv,
}

impl TryFrom<&str> for SimulcastDirection {
    type Error = SimulcastRidParseError;
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "send" => Ok(SimulcastDirection::Send),
            "recv" => Ok(SimulcastDirection::Recv),
            _ => Err(SimulcastRidParseError::UnknownDirection),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SimulcastRid {
    pub(crate) id: String,
    pub(crate) direction: SimulcastDirection,
    pub(crate) params: String,
    pub(crate) paused: bool,
}

impl TryFrom<&String> for SimulcastRid {
    type Error = SimulcastRidParseError;
    fn try_from(value: &String) -> std::result::Result<Self, Self::Error> {
        let mut split = value.split(' ');
        let id = split
            .next()
            .ok_or(SimulcastRidParseError::SyntaxIdDirSplit)?
            .to_owned();
        let direction = SimulcastDirection::try_from(
            split
                .next()
                .ok_or(SimulcastRidParseError::SyntaxIdDirSplit)?,
        )?;
        let params = split.collect();

        Ok(Self {
            id,
            direction,
            params,
            paused: false,
        })
    }
}

fn bundle_match(bundle: Option<&String>, id: &str) -> bool {
    match bundle {
        None => true,
        Some(b) => b.split_whitespace().any(|s| s == id),
    }
}

#[derive(Default)]
pub(crate) struct MediaSection {
    pub(crate) id: String,
    pub(crate) transceivers: Vec<Arc<RTCRtpTransceiver>>,
    pub(crate) data: bool,
    pub(crate) rid_map: Vec<SimulcastRid>,
    pub(crate) offered_direction: Option<RTCRtpTransceiverDirection>,
    pub(crate) extmap_allow_mixed: bool,
}

pub(crate) struct PopulateSdpParams {
    pub(crate) media_description_fingerprint: bool,
    pub(crate) is_icelite: bool,
    pub(crate) extmap_allow_mixed: bool,
    pub(crate) connection_role: ConnectionRole,
    pub(crate) ice_gathering_state: RTCIceGatheringState,
    pub(crate) match_bundle_group: Option<String>,
}

/// populate_sdp serializes a PeerConnections state into an SDP
pub(crate) async fn populate_sdp(
    mut d: SessionDescription,
    dtls_fingerprints: &[RTCDtlsFingerprint],
    media_engine: &Arc<MediaEngine>,
    candidates: &[RTCIceCandidate],
    ice_params: &RTCIceParameters,
    media_sections: &[MediaSection],
    params: PopulateSdpParams,
) -> Result<SessionDescription> {
    let media_dtls_fingerprints = if params.media_description_fingerprint {
        dtls_fingerprints.to_vec()
    } else {
        vec![]
    };

    let mut bundle_value = "BUNDLE".to_owned();
    let mut bundle_count = 0;
    let append_bundle = |mid_value: &str, value: &mut String, count: &mut i32| {
        *value = value.clone() + " " + mid_value;
        *count += 1;
    };

    for (i, m) in media_sections.iter().enumerate() {
        if m.data && !m.transceivers.is_empty() {
            return Err(Error::ErrSDPMediaSectionMediaDataChanInvalid);
        } else if m.transceivers.len() > 1 {
            return Err(Error::ErrSDPMediaSectionMultipleTrackInvalid);
        }

        let should_add_candidates = i == 0;

        let should_add_id = if m.data {
            let params = AddDataMediaSectionParams {
                should_add_candidates,
                mid_value: m.id.clone(),
                ice_params: ice_params.clone(),
                dtls_role: params.connection_role,
                ice_gathering_state: params.ice_gathering_state,
            };
            d = add_data_media_section(d, &media_dtls_fingerprints, candidates, params).await?;
            true
        } else {
            let params = AddTransceiverSdpParams {
                should_add_candidates,
                mid_value: m.id.clone(),
                dtls_role: params.connection_role,
                ice_gathering_state: params.ice_gathering_state,
                offered_direction: m.offered_direction,
            };
            let (d1, should_add_id) = add_transceiver_sdp(
                d,
                &media_dtls_fingerprints,
                media_engine,
                ice_params,
                candidates,
                m,
                params,
            )
            .await?;
            d = d1;
            should_add_id
        };

        if should_add_id {
            if bundle_match(params.match_bundle_group.as_ref(), &m.id) {
                append_bundle(&m.id, &mut bundle_value, &mut bundle_count);
            } else if let Some(desc) = d.media_descriptions.last_mut() {
                desc.media_name.port = RangedPort {
                    value: 0,
                    range: None,
                }
            }
        }
    }

    if !params.media_description_fingerprint {
        for fingerprint in dtls_fingerprints {
            d = d.with_fingerprint(
                fingerprint.algorithm.clone(),
                fingerprint.value.to_uppercase(),
            );
        }
    }

    if params.is_icelite {
        // RFC 5245 S15.3
        d = d.with_value_attribute(ATTR_KEY_ICELITE.to_owned(), ATTR_KEY_ICELITE.to_owned());
    }

    if bundle_count > 0 {
        d = d.with_value_attribute(ATTR_KEY_GROUP.to_owned(), bundle_value);
    }

    if params.extmap_allow_mixed {
        // RFC 8285 6.
        d = d.with_property_attribute(ATTR_KEY_EXTMAP_ALLOW_MIXED.to_owned());
    }

    Ok(d)
}

pub(crate) fn get_mid_value(media: &MediaDescription) -> Option<&String> {
    for attr in &media.attributes {
        if attr.key == "mid" {
            return attr.value.as_ref();
        }
    }
    None
}

pub(crate) fn get_peer_direction(media: &MediaDescription) -> RTCRtpTransceiverDirection {
    for a in &media.attributes {
        let direction = RTCRtpTransceiverDirection::from(a.key.as_str());
        if direction != RTCRtpTransceiverDirection::Unspecified {
            return direction;
        }
    }
    RTCRtpTransceiverDirection::Unspecified
}

pub(crate) fn extract_fingerprint(desc: &SessionDescription) -> Result<(String, String)> {
    let mut fingerprints = vec![];

    if let Some(fingerprint) = desc.attribute("fingerprint") {
        fingerprints.push(fingerprint.clone());
    }

    for m in &desc.media_descriptions {
        if let Some(fingerprint) = m.attribute("fingerprint").and_then(|o| o) {
            fingerprints.push(fingerprint.to_owned());
        }
    }

    if fingerprints.is_empty() {
        return Err(Error::ErrSessionDescriptionNoFingerprint);
    }

    for m in 1..fingerprints.len() {
        if fingerprints[m] != fingerprints[0] {
            return Err(Error::ErrSessionDescriptionConflictingFingerprints);
        }
    }

    let parts: Vec<&str> = fingerprints[0].split(' ').collect();
    if parts.len() != 2 {
        return Err(Error::ErrSessionDescriptionInvalidFingerprint);
    }

    Ok((parts[1].to_owned(), parts[0].to_owned()))
}

pub(crate) async fn extract_ice_details(
    desc: &SessionDescription,
) -> Result<(String, String, Vec<RTCIceCandidate>)> {
    let mut candidates = vec![];

    // Backup ufrag/pwd is the first inactive credentials found.
    // We will return the backup credentials to solve the corner case where
    // all media lines/transceivers are set to inactive.
    //
    // This should probably be handled in a better way by the caller.
    let mut backup_ufrag = None;
    let mut backup_pwd = None;

    let mut remote_ufrag = desc.attribute("ice-ufrag").map(|s| s.as_str());
    let mut remote_pwd = desc.attribute("ice-pwd").map(|s| s.as_str());

    for m in &desc.media_descriptions {
        let ufrag = m.attribute("ice-ufrag").and_then(|o| o);
        let pwd = m.attribute("ice-pwd").and_then(|o| o);

        if m.attribute(ATTR_KEY_INACTIVE).is_some() {
            if backup_ufrag.is_none() {
                backup_ufrag = ufrag;
            }
            if backup_pwd.is_none() {
                backup_pwd = pwd;
            }
            continue;
        }

        if remote_ufrag.is_none() {
            remote_ufrag = ufrag;
        }
        if remote_pwd.is_none() {
            remote_pwd = pwd;
        }

        if ufrag.is_some() && ufrag != remote_ufrag {
            return Err(Error::ErrSessionDescriptionConflictingIceUfrag);
        }
        if pwd.is_some() && pwd != remote_pwd {
            return Err(Error::ErrSessionDescriptionConflictingIcePwd);
        }

        for a in &m.attributes {
            if a.is_ice_candidate() {
                if let Some(value) = &a.value {
                    let c: Arc<dyn Candidate + Send + Sync> = Arc::new(unmarshal_candidate(value)?);
                    let candidate = RTCIceCandidate::from(&c);
                    candidates.push(candidate);
                }
            }
        }
    }

    let remote_ufrag = remote_ufrag
        .or(backup_ufrag)
        .ok_or(Error::ErrSessionDescriptionMissingIceUfrag)?;
    let remote_pwd = remote_pwd
        .or(backup_pwd)
        .ok_or(Error::ErrSessionDescriptionMissingIcePwd)?;

    Ok((remote_ufrag.to_owned(), remote_pwd.to_owned(), candidates))
}

pub(crate) fn have_application_media_section(desc: &SessionDescription) -> bool {
    for m in &desc.media_descriptions {
        if m.media_name.media == MEDIA_SECTION_APPLICATION {
            return true;
        }
    }

    false
}

pub(crate) fn get_by_mid<'a>(
    search_mid: &str,
    desc: &'a session_description::RTCSessionDescription,
) -> Option<&'a MediaDescription> {
    if let Some(parsed) = &desc.parsed {
        for m in &parsed.media_descriptions {
            if let Some(mid) = m.attribute(ATTR_KEY_MID).flatten() {
                if mid == search_mid {
                    return Some(m);
                }
            }
        }
    }
    None
}

/// have_data_channel return MediaDescription with MediaName equal application
pub(crate) fn have_data_channel(
    desc: &session_description::RTCSessionDescription,
) -> Option<&MediaDescription> {
    if let Some(parsed) = &desc.parsed {
        for d in &parsed.media_descriptions {
            if d.media_name.media == MEDIA_SECTION_APPLICATION {
                return Some(d);
            }
        }
    }
    None
}

pub(crate) fn codecs_from_media_description(
    m: &MediaDescription,
) -> Result<Vec<RTCRtpCodecParameters>> {
    let s = SessionDescription {
        media_descriptions: vec![m.clone()],
        ..Default::default()
    };

    let mut out = vec![];
    for payload_str in &m.media_name.formats {
        let payload_type: PayloadType = payload_str.parse::<u8>()?;
        let codec = match s.get_codec_for_payload_type(payload_type) {
            Ok(codec) => codec,
            Err(err) => {
                if payload_type == 0 {
                    continue;
                }
                return Err(err.into());
            }
        };

        let channels = codec.encoding_parameters.parse::<u16>().unwrap_or(0);

        let mut feedback = vec![];
        for raw in &codec.rtcp_feedback {
            let split: Vec<&str> = raw.split(' ').collect();

            let entry = if split.len() == 2 {
                RTCPFeedback {
                    typ: split[0].to_string(),
                    parameter: split[1].to_string(),
                }
            } else {
                RTCPFeedback {
                    typ: split[0].to_string(),
                    parameter: String::new(),
                }
            };

            feedback.push(entry);
        }

        out.push(RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: m.media_name.media.clone() + "/" + codec.name.as_str(),
                clock_rate: codec.clock_rate,
                channels,
                sdp_fmtp_line: codec.fmtp.clone(),
                rtcp_feedback: feedback,
            },
            payload_type,
            stats_id: String::new(),
        })
    }

    Ok(out)
}

pub(crate) fn rtp_extensions_from_media_description(
    m: &MediaDescription,
) -> Result<HashMap<String, isize>> {
    let mut out = HashMap::new();

    for a in &m.attributes {
        if a.key == ATTR_KEY_EXT_MAP {
            let a_str = a.to_string();
            let mut reader = BufReader::new(a_str.as_bytes());
            let e = ExtMap::unmarshal(&mut reader)?;

            if let Some(uri) = e.uri {
                out.insert(uri.to_string(), e.value);
            }
        }
    }

    Ok(out)
}

/// update_sdp_origin saves sdp.Origin in PeerConnection when creating 1st local SDP;
/// for subsequent calling, it updates Origin for SessionDescription from saved one
/// and increments session version by one.
/// <https://tools.ietf.org/html/draft-ietf-rtcweb-jsep-25#section-5.2.2>
pub(crate) fn update_sdp_origin(origin: &mut Origin, d: &mut SessionDescription) {
    //TODO: if atomic.CompareAndSwapUint64(&origin.SessionVersion, 0, d.Origin.SessionVersion)
    if origin.session_version == 0 {
        // store
        origin.session_version = d.origin.session_version;
        //atomic.StoreUint64(&origin.SessionID, d.Origin.SessionID)
        origin.session_id = d.origin.session_id;
    } else {
        // load
        /*for { // awaiting for saving session id
            d.Origin.SessionID = atomic.LoadUint64(&origin.SessionID)
            if d.Origin.SessionID != 0 {
                break
            }
        }*/
        d.origin.session_id = origin.session_id;

        //d.Origin.SessionVersion = atomic.AddUint64(&origin.SessionVersion, 1)
        origin.session_version += 1;
        d.origin.session_version += 1;
    }
}
