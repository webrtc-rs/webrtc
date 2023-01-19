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
use crate::rtp_transceiver::RTCRtpTransceiver;
use crate::rtp_transceiver::{PayloadType, RTCPFeedback, SSRC};

pub mod sdp_type;
pub mod session_description;

use crate::peer_connection::MEDIA_SECTION_APPLICATION;
use crate::SDP_ATTRIBUTE_RID;
use ice::candidate::candidate_base::unmarshal_candidate;
use ice::candidate::Candidate;
use sdp::description::common::{Address, ConnectionInformation};
use sdp::description::media::{MediaDescription, MediaName, RangedPort};
use sdp::description::session::*;
use sdp::extmap::ExtMap;
use sdp::util::ConnectionRole;
use std::collections::HashMap;
use std::convert::From;
use std::io::BufReader;
use std::sync::Arc;
use url::Url;

/// TrackDetails represents any media source that can be represented in a SDP
/// This isn't keyed by SSRC because it also needs to support rid based sources
#[derive(Default, Debug, Clone)]
pub(crate) struct TrackDetails {
    pub(crate) mid: String,
    pub(crate) kind: RTPCodecType,
    pub(crate) stream_id: String,
    pub(crate) id: String,
    pub(crate) ssrcs: Vec<SSRC>,
    pub(crate) repair_ssrc: SSRC,
    pub(crate) rids: Vec<String>,
}

pub(crate) fn track_details_for_ssrc(
    track_details: &[TrackDetails],
    ssrc: SSRC,
) -> Option<&TrackDetails> {
    track_details.iter().find(|x| x.ssrcs.contains(&ssrc))
}

pub(crate) fn track_details_for_rid(
    track_details: &[TrackDetails],
    rid: String,
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

                        let mut repair_ssrc = 0;
                        for (repair, base) in &rtx_repair_flows {
                            if *base == ssrc {
                                repair_ssrc = *repair;
                                //TODO: no break?
                            }
                        }

                        if track_idx < tracks_in_media_section.len() {
                            tracks_in_media_section[track_idx].mid = mid_value.to_owned();
                            tracks_in_media_section[track_idx].kind = codec_type;
                            tracks_in_media_section[track_idx].stream_id = stream_id.to_owned();
                            tracks_in_media_section[track_idx].id = track_id.to_owned();
                            tracks_in_media_section[track_idx].ssrcs = vec![ssrc];
                            tracks_in_media_section[track_idx].repair_ssrc = repair_ssrc;
                        } else {
                            let track_details = TrackDetails {
                                mid: mid_value.to_owned(),
                                kind: codec_type,
                                stream_id: stream_id.to_owned(),
                                id: track_id.to_owned(),
                                ssrcs: vec![ssrc],
                                repair_ssrc,
                                ..Default::default()
                            };
                            tracks_in_media_section.push(track_details);
                        }
                    }
                }
                _ => {}
            };
        }

        let rids = get_rids(media);
        if !rids.is_empty() && !track_id.is_empty() && !stream_id.is_empty() {
            let mut simulcast_track = TrackDetails {
                mid: mid_value.to_owned(),
                kind: codec_type,
                stream_id: stream_id.to_owned(),
                id: track_id.to_owned(),
                rids: vec![],
                ..Default::default()
            };
            for rid in rids.keys() {
                simulcast_track.rids.push(rid.to_owned());
            }
            if simulcast_track.rids.len() == tracks_in_media_section.len() {
                for track in &tracks_in_media_section {
                    simulcast_track.ssrcs.extend(&track.ssrcs)
                }
            }

            tracks_in_media_section = vec![simulcast_track];
        }

        incoming_tracks.extend(tracks_in_media_section);
    }

    incoming_tracks
}

pub(crate) fn get_rids(media: &MediaDescription) -> HashMap<String, String> {
    let mut rids = HashMap::new();
    for attr in &media.attributes {
        if attr.key.as_str() == SDP_ATTRIBUTE_RID {
            if let Some(value) = &attr.value {
                let split: Vec<&str> = value.split(' ').collect();
                rids.insert(split[0].to_owned(), value.to_owned());
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
        if t.sender().track().await.is_some() {
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
        let mut recv_rids: Vec<String> = vec![];

        for rid in media_section.rid_map.keys() {
            media =
                media.with_value_attribute(SDP_ATTRIBUTE_RID.to_owned(), rid.to_owned() + " recv");
            recv_rids.push(rid.to_owned());
        }
        // Simulcast
        media = media.with_value_attribute(
            "simulcast".to_owned(),
            "recv ".to_owned() + recv_rids.join(";").as_str(),
        );
    }

    for mt in transceivers {
        let sender = mt.sender();
        if let Some(track) = sender.track().await {
            media = media.with_media_source(
                sender.ssrc,
                track.stream_id().to_owned(), /* cname */
                track.stream_id().to_owned(), /* streamLabel */
                track.id().to_owned(),
            );

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

#[derive(Default)]
pub(crate) struct MediaSection {
    pub(crate) id: String,
    pub(crate) transceivers: Vec<Arc<RTCRtpTransceiver>>,
    pub(crate) data: bool,
    pub(crate) rid_map: HashMap<String, String>,
    pub(crate) offered_direction: Option<RTCRtpTransceiverDirection>,
}

pub(crate) struct PopulateSdpParams {
    pub(crate) media_description_fingerprint: bool,
    pub(crate) is_icelite: bool,
    pub(crate) connection_role: ConnectionRole,
    pub(crate) ice_gathering_state: RTCIceGatheringState,
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
            append_bundle(&m.id, &mut bundle_value, &mut bundle_count);
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

    Ok(d.with_value_attribute(ATTR_KEY_GROUP.to_owned(), bundle_value))
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
    let mut remote_pwds = vec![];
    let mut remote_ufrags = vec![];

    if let Some(ufrag) = desc.attribute("ice-ufrag") {
        remote_ufrags.push(ufrag.clone());
    }
    if let Some(pwd) = desc.attribute("ice-pwd") {
        remote_pwds.push(pwd.clone());
    }

    for m in &desc.media_descriptions {
        if let Some(ufrag) = m.attribute("ice-ufrag").and_then(|o| o) {
            remote_ufrags.push(ufrag.to_owned());
        }
        if let Some(pwd) = m.attribute("ice-pwd").and_then(|o| o) {
            remote_pwds.push(pwd.to_owned());
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

    if remote_ufrags.is_empty() {
        return Err(Error::ErrSessionDescriptionMissingIceUfrag);
    } else if remote_pwds.is_empty() {
        return Err(Error::ErrSessionDescriptionMissingIcePwd);
    }

    for m in 1..remote_ufrags.len() {
        if remote_ufrags[m] != remote_ufrags[0] {
            return Err(Error::ErrSessionDescriptionConflictingIceUfrag);
        }
    }

    for m in 1..remote_pwds.len() {
        if remote_pwds[m] != remote_pwds[0] {
            return Err(Error::ErrSessionDescriptionConflictingIcePwd);
        }
    }

    Ok((remote_ufrags[0].clone(), remote_pwds[0].clone(), candidates))
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
