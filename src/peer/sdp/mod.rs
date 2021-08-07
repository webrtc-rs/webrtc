use crate::media::rtp::rtp_codec::RTPCodecType;
use crate::media::rtp::SSRC;

use sdp::media_description::MediaDescription;
use sdp::session_description::*;
use std::collections::HashMap;

pub mod sdp_type;
pub mod session_description;

/// TrackDetails represents any media source that can be represented in a SDP
/// This isn't keyed by SSRC because it also needs to support rid based sources
#[derive(Default, Debug, Clone)]
pub(crate) struct TrackDetails {
    mid: String,
    kind: RTPCodecType,
    stream_id: String,
    id: String,
    ssrc: SSRC,
    rids: Vec<String>,
}

pub(crate) fn track_details_for_ssrc(
    track_details: &[TrackDetails],
    ssrc: SSRC,
) -> Option<&TrackDetails> {
    track_details.iter().find(|x| x.ssrc == ssrc)
}

pub(crate) fn filter_track_with_ssrc(incoming_tracks: &mut Vec<TrackDetails>, ssrc: SSRC) {
    incoming_tracks.retain(|x| x.ssrc != ssrc);
}

/// extract all TrackDetails from an SDP.
pub(crate) fn track_details_from_sdp(
    s: &sdp::session_description::SessionDescription,
) -> Vec<TrackDetails> {
    let mut incoming_tracks = vec![];
    let mut rtx_repair_flows = HashMap::new();

    for media in &s.media_descriptions {
        // Plan B can have multiple tracks in a signle media section
        let mut stream_id = "";
        let mut track_id = "";

        // If media section is recvonly or inactive skip
        if media.attribute(ATTR_KEY_RECV_ONLY).is_some()
            || media.attribute(ATTR_KEY_INACTIVE).is_some()
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
                                if let Err(err) = split[1].parse::<u32>() {
                                    log::warn!("Failed to parse SSRC: {}", err);
                                    continue;
                                }
                                let rtx_repair_flow = match split[2].parse::<u32>() {
                                    Ok(n) => n,
                                    Err(err) => {
                                        log::warn!("Failed to parse SSRC: {}", err);
                                        continue;
                                    }
                                };
                                rtx_repair_flows.insert(rtx_repair_flow, true);
                                // Remove if rtx was added as track before
                                filter_track_with_ssrc(
                                    &mut incoming_tracks,
                                    rtx_repair_flow as SSRC,
                                );
                            }
                        }
                    }
                }

                // Handle `a=msid:<stream_id> <track_label>` for Unified plan. The first value is the same as MediaStream.id
                // in the browser and can be used to figure out which tracks belong to the same stream. The browser should
                // figure this out automatically when an ontrack event is emitted on RTCPeerConnection.
                ATTR_KEY_MSID => {
                    if let Some(value) = &attr.value {
                        let split: Vec<&str> = value.split(' ').collect();
                        if split.len() == 2 {
                            stream_id = split[0];
                            track_id = split[1];
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

                        let mut track_idx = incoming_tracks.len();

                        for (i, t) in incoming_tracks.iter().enumerate() {
                            if t.ssrc == ssrc {
                                track_idx = i;
                                //TODO: no break?
                            }
                        }

                        if track_idx < incoming_tracks.len() {
                            incoming_tracks[track_idx].mid = mid_value.to_owned();
                            incoming_tracks[track_idx].kind = codec_type;
                            incoming_tracks[track_idx].stream_id = stream_id.to_owned();
                            incoming_tracks[track_idx].id = track_id.to_owned();
                            incoming_tracks[track_idx].ssrc = ssrc;
                        } else {
                            let track_details = TrackDetails {
                                mid: mid_value.to_owned(),
                                kind: codec_type,
                                stream_id: stream_id.to_owned(),
                                id: track_id.to_owned(),
                                ssrc,
                                ..Default::default()
                            };
                            incoming_tracks.push(track_details);
                        }
                    }
                }
                _ => {}
            };
        }

        let rids = get_rids(media);
        if !rids.is_empty() && !track_id.is_empty() && !stream_id.is_empty() {
            let mut new_track = TrackDetails {
                mid: mid_value.to_owned(),
                kind: codec_type,
                stream_id: stream_id.to_owned(),
                id: track_id.to_owned(),
                rids: vec![],
                ..Default::default()
            };
            for rid in rids.keys() {
                new_track.rids.push(rid.to_owned());
            }

            incoming_tracks.push(new_track);
        }
    }

    incoming_tracks
}

fn get_rids(media: &MediaDescription) -> HashMap<String, String> {
    let mut rids = HashMap::new();
    for attr in &media.attributes {
        if attr.key.as_str() == "rid" {
            if let Some(value) = &attr.value {
                let split: Vec<&str> = value.split(' ').collect();
                rids.insert(split[0].to_owned(), value.to_owned());
            }
        }
    }
    rids
}
/*
func addCandidatesToMediaDescriptions(candidates []ICECandidate, m *sdp.MediaDescription, iceGatheringState ICEGatheringState) error {
    appendCandidateIfNew := func(c ice.Candidate, attributes []sdp.Attribute) {
        marshaled := c.Marshal()
        for _, a := range attributes {
            if marshaled == a.Value {
                return
            }
        }

        m.WithValueAttribute("candidate", marshaled)
    }

    for _, c := range candidates {
        candidate, err := c.toICE()
        if err != nil {
            return err
        }

        candidate.SetComponent(1)
        appendCandidateIfNew(candidate, m.Attributes)

        candidate.SetComponent(2)
        appendCandidateIfNew(candidate, m.Attributes)
    }

    if iceGatheringState != ICEGatheringStateComplete {
        return nil
    }
    for _, a := range m.Attributes {
        if a.Key == "end-of-candidates" {
            return nil
        }
    }

    m.WithPropertyAttribute("end-of-candidates")
    return nil
}

func addDataMediaSection(d *sdp.SessionDescription, shouldAddCandidates bool, dtlsFingerprints []DTLSFingerprint, midValue string, iceParams ICEParameters, candidates []ICECandidate, dtlsRole sdp.ConnectionRole, iceGatheringState ICEGatheringState) error {
    media := (&sdp.MediaDescription{
        MediaName: sdp.MediaName{
            Media:   mediaSectionApplication,
            Port:    sdp.RangedPort{Value: 9},
            Protos:  []string{"UDP", "DTLS", "SCTP"},
            Formats: []string{"webrtc-datachannel"},
        },
        ConnectionInformation: &sdp.ConnectionInformation{
            NetworkType: "IN",
            AddressType: "IP4",
            Address: &sdp.Address{
                Address: "0.0.0.0",
            },
        },
    }).
        WithValueAttribute(sdp.AttrKeyConnectionSetup, dtlsRole.String()).
        WithValueAttribute(sdp.AttrKeyMID, midValue).
        WithPropertyAttribute(RTPTransceiverDirectionSendrecv.String()).
        WithPropertyAttribute("sctp-port:5000").
        WithICECredentials(iceParams.UsernameFragment, iceParams.Password)

    for _, f := range dtlsFingerprints {
        media = media.WithFingerprint(f.Algorithm, strings.ToUpper(f.Value))
    }

    if shouldAddCandidates {
        if err := addCandidatesToMediaDescriptions(candidates, media, iceGatheringState); err != nil {
            return err
        }
    }

    d.WithMedia(media)
    return nil
}

func populateLocalCandidates(sessionDescription *SessionDescription, i *ICEGatherer, iceGatheringState ICEGatheringState) *SessionDescription {
    if sessionDescription == nil || i == nil {
        return sessionDescription
    }

    candidates, err := i.GetLocalCandidates()
    if err != nil {
        return sessionDescription
    }

    parsed := sessionDescription.parsed
    if len(parsed.MediaDescriptions) > 0 {
        m := parsed.MediaDescriptions[0]
        if err = addCandidatesToMediaDescriptions(candidates, m, iceGatheringState); err != nil {
            return sessionDescription
        }
    }

    sdp, err := parsed.Marshal()
    if err != nil {
        return sessionDescription
    }

    return &SessionDescription{
        SDP:    string(sdp),
        Type:   sessionDescription.Type,
        parsed: parsed,
    }
}

func addTransceiverSDP(d *sdp.SessionDescription, isPlanB, shouldAddCandidates bool, dtlsFingerprints []DTLSFingerprint, mediaEngine *MediaEngine, midValue string, iceParams ICEParameters, candidates []ICECandidate, dtlsRole sdp.ConnectionRole, iceGatheringState ICEGatheringState, mediaSection mediaSection) (bool, error) {
    transceivers := mediaSection.transceivers
    if len(transceivers) < 1 {
        return false, errSDPZeroTransceivers
    }
    // Use the first transceiver to generate the section attributes
    t := transceivers[0]
    media := sdp.NewJSEPMediaDescription(t.kind.String(), []string{}).
        WithValueAttribute(sdp.AttrKeyConnectionSetup, dtlsRole.String()).
        WithValueAttribute(sdp.AttrKeyMID, midValue).
        WithICECredentials(iceParams.UsernameFragment, iceParams.Password).
        WithPropertyAttribute(sdp.AttrKeyRTCPMux).
        WithPropertyAttribute(sdp.AttrKeyRTCPRsize)

    codecs := t.getCodecs()
    for _, codec := range codecs {
        name := strings.TrimPrefix(codec.MimeType, "audio/")
        name = strings.TrimPrefix(name, "video/")
        media.WithCodec(uint8(codec.PayloadType), name, codec.ClockRate, codec.Channels, codec.SDPFmtpLine)

        for _, feedback := range codec.RTPCodecCapability.RTCPFeedback {
            media.WithValueAttribute("rtcp-fb", fmt.Sprintf("%d %s %s", codec.PayloadType, feedback.Type, feedback.Parameter))
        }
    }
    if len(codecs) == 0 {
        // Explicitly reject track if we don't have the codec
        d.WithMedia(&sdp.MediaDescription{
            MediaName: sdp.MediaName{
                Media:   t.kind.String(),
                Port:    sdp.RangedPort{Value: 0},
                Protos:  []string{"UDP", "TLS", "RTP", "SAVPF"},
                Formats: []string{"0"},
            },
        })
        return false, nil
    }

    directions := []RTPTransceiverDirection{}
    if t.Sender() != nil {
        directions = append(directions, RTPTransceiverDirectionSendonly)
    }
    if t.Receiver() != nil {
        directions = append(directions, RTPTransceiverDirectionRecvonly)
    }

    parameters := mediaEngine.getRTPParametersByKind(t.kind, directions)
    for _, rtpExtension := range parameters.HeaderExtensions {
        extURL, err := url.Parse(rtpExtension.URI)
        if err != nil {
            return false, err
        }
        media.WithExtMap(sdp.ExtMap{Value: rtpExtension.ID, URI: extURL})
    }

    if len(mediaSection.ridMap) > 0 {
        recvRids := make([]string, 0, len(mediaSection.ridMap))

        for rid := range mediaSection.ridMap {
            media.WithValueAttribute("rid", rid+" recv")
            recvRids = append(recvRids, rid)
        }
        // Simulcast
        media.WithValueAttribute("simulcast", "recv "+strings.Join(recvRids, ";"))
    }

    for _, mt := range transceivers {
        if mt.Sender() != nil && mt.Sender().Track() != nil {
            track := mt.Sender().Track()
            media = media.WithMediaSource(uint32(mt.Sender().ssrc), track.StreamID() /* cname */, track.StreamID() /* streamLabel */, track.ID())
            if !isPlanB {
                media = media.WithPropertyAttribute("msid:" + track.StreamID() + " " + track.ID())
                break
            }
        }
    }

    media = media.WithPropertyAttribute(t.Direction().String())

    for _, fingerprint := range dtlsFingerprints {
        media = media.WithFingerprint(fingerprint.Algorithm, strings.ToUpper(fingerprint.Value))
    }

    if shouldAddCandidates {
        if err := addCandidatesToMediaDescriptions(candidates, media, iceGatheringState); err != nil {
            return false, err
        }
    }

    d.WithMedia(media)

    return true, nil
}

type mediaSection struct {
    id           string
    transceivers []*RTPTransceiver
    data         bool
    ridMap       map[string]string
}

// populateSDP serializes a PeerConnections state into an SDP
func populateSDP(d *sdp.SessionDescription, isPlanB bool, dtlsFingerprints []DTLSFingerprint, mediaDescriptionFingerprint bool, isICELite bool, mediaEngine *MediaEngine, connectionRole sdp.ConnectionRole, candidates []ICECandidate, iceParams ICEParameters, mediaSections []mediaSection, iceGatheringState ICEGatheringState) (*sdp.SessionDescription, error) {
    var err error
    mediaDtlsFingerprints := []DTLSFingerprint{}

    if mediaDescriptionFingerprint {
        mediaDtlsFingerprints = dtlsFingerprints
    }

    bundleValue := "BUNDLE"
    bundleCount := 0
    appendBundle := func(midValue string) {
        bundleValue += " " + midValue
        bundleCount++
    }

    for i, m := range mediaSections {
        if m.data && len(m.transceivers) != 0 {
            return nil, errSDPMediaSectionMediaDataChanInvalid
        } else if !isPlanB && len(m.transceivers) > 1 {
            return nil, errSDPMediaSectionMultipleTrackInvalid
        }

        shouldAddID := true
        shouldAddCandidates := i == 0
        if m.data {
            if err = addDataMediaSection(d, shouldAddCandidates, mediaDtlsFingerprints, m.id, iceParams, candidates, connectionRole, iceGatheringState); err != nil {
                return nil, err
            }
        } else {
            shouldAddID, err = addTransceiverSDP(d, isPlanB, shouldAddCandidates, mediaDtlsFingerprints, mediaEngine, m.id, iceParams, candidates, connectionRole, iceGatheringState, m)
            if err != nil {
                return nil, err
            }
        }

        if shouldAddID {
            appendBundle(m.id)
        }
    }

    if !mediaDescriptionFingerprint {
        for _, fingerprint := range dtlsFingerprints {
            d.WithFingerprint(fingerprint.Algorithm, strings.ToUpper(fingerprint.Value))
        }
    }

    if isICELite {
        // RFC 5245 S15.3
        d = d.WithValueAttribute(sdp.AttrKeyICELite, sdp.AttrKeyICELite)
    }

    return d.WithValueAttribute(sdp.AttrKeyGroup, bundleValue), nil
}
*/

fn get_mid_value(media: &MediaDescription) -> Option<&String> {
    for attr in &media.attributes {
        if attr.key == "mid" {
            return attr.value.as_ref();
        }
    }
    None
}

/*
func descriptionIsPlanB(desc *SessionDescription) bool {
    if desc == nil || desc.parsed == nil {
        return false
    }

    detectionRegex := regexp.MustCompile(`(?i)^(audio|video|data)$`)
    for _, media := range desc.parsed.MediaDescriptions {
        if len(detectionRegex.FindStringSubmatch(get_mid_value(media))) == 2 {
            return true
        }
    }
    return false
}

func getPeerDirection(media *sdp.MediaDescription) RTPTransceiverDirection {
    for _, a := range media.Attributes {
        if direction := NewRTPTransceiverDirection(a.Key); direction != RTPTransceiverDirection(Unknown) {
            return direction
        }
    }
    return RTPTransceiverDirection(Unknown)
}

func extractFingerprint(desc *sdp.SessionDescription) (string, string, error) {
    fingerprints := []string{}

    if fingerprint, haveFingerprint := desc.Attribute("fingerprint"); haveFingerprint {
        fingerprints = append(fingerprints, fingerprint)
    }

    for _, m := range desc.MediaDescriptions {
        if fingerprint, haveFingerprint := m.Attribute("fingerprint"); haveFingerprint {
            fingerprints = append(fingerprints, fingerprint)
        }
    }

    if len(fingerprints) < 1 {
        return "", "", ErrSessionDescriptionNoFingerprint
    }

    for _, m := range fingerprints {
        if m != fingerprints[0] {
            return "", "", ErrSessionDescriptionConflictingFingerprints
        }
    }

    parts := strings.Split(fingerprints[0], " ")
    if len(parts) != 2 {
        return "", "", ErrSessionDescriptionInvalidFingerprint
    }
    return parts[1], parts[0], nil
}

func extractICEDetails(desc *sdp.SessionDescription) (string, string, []ICECandidate, error) {
    candidates := []ICECandidate{}
    remotePwds := []string{}
    remoteUfrags := []string{}

    if ufrag, haveUfrag := desc.Attribute("ice-ufrag"); haveUfrag {
        remoteUfrags = append(remoteUfrags, ufrag)
    }
    if pwd, havePwd := desc.Attribute("ice-pwd"); havePwd {
        remotePwds = append(remotePwds, pwd)
    }

    for _, m := range desc.MediaDescriptions {
        if ufrag, haveUfrag := m.Attribute("ice-ufrag"); haveUfrag {
            remoteUfrags = append(remoteUfrags, ufrag)
        }
        if pwd, havePwd := m.Attribute("ice-pwd"); havePwd {
            remotePwds = append(remotePwds, pwd)
        }

        for _, a := range m.Attributes {
            if a.IsICECandidate() {
                c, err := ice.UnmarshalCandidate(a.Value)
                if err != nil {
                    return "", "", nil, err
                }

                candidate, err := newICECandidateFromICE(c)
                if err != nil {
                    return "", "", nil, err
                }

                candidates = append(candidates, candidate)
            }
        }
    }

    if len(remoteUfrags) == 0 {
        return "", "", nil, ErrSessionDescriptionMissingIceUfrag
    } else if len(remotePwds) == 0 {
        return "", "", nil, ErrSessionDescriptionMissingIcePwd
    }

    for _, m := range remoteUfrags {
        if m != remoteUfrags[0] {
            return "", "", nil, ErrSessionDescriptionConflictingIceUfrag
        }
    }

    for _, m := range remotePwds {
        if m != remotePwds[0] {
            return "", "", nil, ErrSessionDescriptionConflictingIcePwd
        }
    }

    return remoteUfrags[0], remotePwds[0], candidates, nil
}

func haveApplicationMediaSection(desc *sdp.SessionDescription) bool {
    for _, m := range desc.MediaDescriptions {
        if m.MediaName.Media == mediaSectionApplication {
            return true
        }
    }

    return false
}

func getByMid(searchMid string, desc *SessionDescription) *sdp.MediaDescription {
    for _, m := range desc.parsed.MediaDescriptions {
        if mid, ok := m.Attribute(sdp.AttrKeyMID); ok && mid == searchMid {
            return m
        }
    }
    return nil
}

// haveDataChannel return MediaDescription with MediaName equal application
func haveDataChannel(desc *SessionDescription) *sdp.MediaDescription {
    for _, d := range desc.parsed.MediaDescriptions {
        if d.MediaName.Media == mediaSectionApplication {
            return d
        }
    }
    return nil
}

func codecsFromMediaDescription(m *sdp.MediaDescription) (out []RTPCodecParameters, err error) {
    s := &sdp.SessionDescription{
        MediaDescriptions: []*sdp.MediaDescription{m},
    }

    for _, payloadStr := range m.MediaName.Formats {
        payloadType, err := strconv.Atoi(payloadStr)
        if err != nil {
            return nil, err
        }

        codec, err := s.GetCodecForPayloadType(uint8(payloadType))
        if err != nil {
            if payloadType == 0 {
                continue
            }
            return nil, err
        }

        channels := uint16(0)
        val, err := strconv.Atoi(codec.EncodingParameters)
        if err == nil {
            channels = uint16(val)
        }

        feedback := []RTCPFeedback{}
        for _, raw := range codec.RTCPFeedback {
            split := strings.Split(raw, " ")
            entry := RTCPFeedback{Type: split[0]}
            if len(split) == 2 {
                entry.Parameter = split[1]
            }

            feedback = append(feedback, entry)
        }

        out = append(out, RTPCodecParameters{
            RTPCodecCapability: RTPCodecCapability{m.MediaName.Media + "/" + codec.Name, codec.ClockRate, channels, codec.Fmtp, feedback},
            PayloadType:        PayloadType(payloadType),
        })
    }

    return out, nil
}

func rtpExtensionsFromMediaDescription(m *sdp.MediaDescription) (map[string]int, error) {
    out := map[string]int{}

    for _, a := range m.Attributes {
        if a.Key == sdp.AttrKeyExtMap {
            e := sdp.ExtMap{}
            if err := e.Unmarshal(a.String()); err != nil {
                return nil, err
            }

            out[e.URI.String()] = e.Value
        }
    }

    return out, nil
}

// updateSDPOrigin saves sdp.Origin in PeerConnection when creating 1st local SDP;
// for subsequent calling, it updates Origin for SessionDescription from saved one
// and increments session version by one.
// https://tools.ietf.org/html/draft-ietf-rtcweb-jsep-25#section-5.2.2
// https://tools.ietf.org/html/draft-ietf-rtcweb-jsep-25#section-5.3.2
func updateSDPOrigin(origin *sdp.Origin, d *sdp.SessionDescription) {
    if atomic.CompareAndSwapUint64(&origin.SessionVersion, 0, d.Origin.SessionVersion) { // store
        atomic.StoreUint64(&origin.SessionID, d.Origin.SessionID)
    } else { // load
        for { // awaiting for saving session id
            d.Origin.SessionID = atomic.LoadUint64(&origin.SessionID)
            if d.Origin.SessionID != 0 {
                break
            }
        }
        d.Origin.SessionVersion = atomic.AddUint64(&origin.SessionVersion, 1)
    }
}
*/
