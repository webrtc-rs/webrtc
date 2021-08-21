use serde::{Deserialize, Serialize};
use std::fmt;

/// SDPSemantics determines which style of SDP offers and answers
/// can be used
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum SDPSemantics {
    Unspecified = 0,

    /// UnifiedPlan uses unified-plan offers and answers
    /// (the default in Chrome since M72)
    /// https://tools.ietf.org/html/draft-roach-mmusic-unified-plan-00
    UnifiedPlan = 1,

    /// PlanB uses plan-b offers and answers
    /// NB: This format should be considered deprecated
    /// https://tools.ietf.org/html/draft-uberti-rtcweb-plan-00
    PlanB = 2,

    /// UnifiedPlanWithFallback prefers unified-plan
    /// offers and answers, but will respond to a plan-b offer
    /// with a plan-b answer
    UnifiedPlanWithFallback = 3,
}

impl Default for SDPSemantics {
    fn default() -> Self {
        SDPSemantics::Unspecified
    }
}

const SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK: &str = "UnifiedPlanWithFallback";
const SDP_SEMANTICS_UNIFIED_PLAN: &str = "UnifiedPlan";
const SDP_SEMANTICS_PLAN_B: &str = "PlanB";

impl From<&str> for SDPSemantics {
    fn from(raw: &str) -> Self {
        match raw {
            SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK => SDPSemantics::UnifiedPlanWithFallback,
            SDP_SEMANTICS_UNIFIED_PLAN => SDPSemantics::UnifiedPlan,
            SDP_SEMANTICS_PLAN_B => SDPSemantics::PlanB,
            _ => SDPSemantics::Unspecified,
        }
    }
}

impl fmt::Display for SDPSemantics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            SDPSemantics::UnifiedPlanWithFallback => SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK,
            SDPSemantics::UnifiedPlan => SDP_SEMANTICS_UNIFIED_PLAN,
            SDPSemantics::PlanB => SDP_SEMANTICS_PLAN_B,
            SDPSemantics::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::SSRC_STR;
    use sdp::media_description::MediaDescription;
    use sdp::session_description::SessionDescription;
    use std::collections::HashSet;

    #[test]
    fn test_sdp_semantics_string() {
        let tests = vec![
            (SDPSemantics::Unspecified, "Unspecified"),
            (
                SDPSemantics::UnifiedPlanWithFallback,
                "UnifiedPlanWithFallback",
            ),
            (SDPSemantics::PlanB, "PlanB"),
            (SDPSemantics::UnifiedPlan, "UnifiedPlan"),
        ];

        for (value, expected_string) in tests {
            assert_eq!(expected_string, value.to_string());
        }
    }

    // The following tests are for non-standard SDP semantics
    // (i.e. not unified-unified)
    fn get_md_names(sdp: &SessionDescription) -> Vec<String> {
        sdp.media_descriptions
            .iter()
            .map(|md| md.media_name.media.clone())
            .collect()
    }

    fn extract_ssrc_list(md: &MediaDescription) -> Vec<String> {
        let mut ssrcs = HashSet::new();
        for attr in &md.attributes {
            if attr.key == SSRC_STR {
                if let Some(value) = &attr.value {
                    let fields: Vec<&str> = value.split_whitespace().collect();
                    if let Some(ssrc) = fields.first() {
                        ssrcs.insert(*ssrc);
                    }
                }
            }
        }
        ssrcs
            .into_iter()
            .map(|ssrc| ssrc.to_owned())
            .collect::<Vec<String>>()
    }

    /*TODO:
    func TestSDPSemantics_PlanBOfferTransceivers(t *testing.T) {
        report := test.CheckRoutines(t)
        defer report()

        lim := test.TimeOut(time.Second * 30)
        defer lim.Stop()

        opc, err := new_peer_connection(Configuration{
            SDPSemantics: SDPSemanticsPlanB,
        })
        if err != nil {
            t.Errorf("new_peer_connection failed: %v", err)
        }

        if _, err = opc.AddTransceiverFromKind(RTPCodecTypeVideo, RTPTransceiverInit{
            Direction: RTPTransceiverDirectionSendrecv,
        }); err != nil {
            t.Errorf("AddTransceiver failed: %v", err)
        }
        if _, err = opc.AddTransceiverFromKind(RTPCodecTypeVideo, RTPTransceiverInit{
            Direction: RTPTransceiverDirectionSendrecv,
        }); err != nil {
            t.Errorf("AddTransceiver failed: %v", err)
        }
        if _, err = opc.AddTransceiverFromKind(RTPCodecTypeAudio, RTPTransceiverInit{
            Direction: RTPTransceiverDirectionSendrecv,
        }); err != nil {
            t.Errorf("AddTransceiver failed: %v", err)
        }
        if _, err = opc.AddTransceiverFromKind(RTPCodecTypeAudio, RTPTransceiverInit{
            Direction: RTPTransceiverDirectionSendrecv,
        }); err != nil {
            t.Errorf("AddTransceiver failed: %v", err)
        }

        offer, err := opc.CreateOffer(nil)
        if err != nil {
            t.Errorf("Plan B CreateOffer failed: %s", err)
        }

        mdNames := getMdNames(offer.parsed)
        assert.ObjectsAreEqual(mdNames, []string{"video", "audio", "data"})

        // Verify that each section has 2 SSRCs (one for each transceiver)
        for _, section := range []string{"video", "audio"} {
            for _, media := range offer.parsed.MediaDescriptions {
                if media.MediaName.Media == section {
                    assert.Len(t, extractSsrcList(media), 2)
                }
            }
        }

        apc, err := new_peer_connection(Configuration{
            SDPSemantics: SDPSemanticsPlanB,
        })
        if err != nil {
            t.Errorf("new_peer_connection failed: %v", err)
        }

        if err = apc.set_remote_description(offer); err != nil {
            t.Errorf("set_remote_description failed: %s", err)
        }

        answer, err := apc.create_answer(nil)
        if err != nil {
            t.Errorf("Plan B create_answer failed: %s", err)
        }

        mdNames = getMdNames(answer.parsed)
        assert.ObjectsAreEqual(mdNames, []string{"video", "audio", "data"})

        closePairNow(t, apc, opc)
    }

    func TestSDPSemantics_PlanBAnswerSenders(t *testing.T) {
        report := test.CheckRoutines(t)
        defer report()

        lim := test.TimeOut(time.Second * 30)
        defer lim.Stop()

        opc, err := new_peer_connection(Configuration{
            SDPSemantics: SDPSemanticsPlanB,
        })
        if err != nil {
            t.Errorf("new_peer_connection failed: %v", err)
        }

        if _, err = opc.AddTransceiverFromKind(RTPCodecTypeVideo, RTPTransceiverInit{
            Direction: RTPTransceiverDirectionRecvonly,
        }); err != nil {
            t.Errorf("Failed to add transceiver")
        }
        if _, err = opc.AddTransceiverFromKind(RTPCodecTypeAudio, RTPTransceiverInit{
            Direction: RTPTransceiverDirectionRecvonly,
        }); err != nil {
            t.Errorf("Failed to add transceiver")
        }

        offer, err := opc.CreateOffer(nil)
        if err != nil {
            t.Errorf("Plan B CreateOffer failed: %s", err)
        }

        mdNames := getMdNames(offer.parsed)
        assert.ObjectsAreEqual(mdNames, []string{"video", "audio", "data"})

        apc, err := new_peer_connection(Configuration{
            SDPSemantics: SDPSemanticsPlanB,
        })
        if err != nil {
            t.Errorf("new_peer_connection failed: %v", err)
        }

        video1, err := NewTrackLocalStaticSample(RTPCodecCapability{MimeType: "video/h264", SDPFmtpLine: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"}, "1", "1")
        if err != nil {
            t.Errorf("Failed to create video track")
        }
        if _, err = apc.AddTrack(video1); err != nil {
            t.Errorf("Failed to add video track")
        }
        video2, err := NewTrackLocalStaticSample(RTPCodecCapability{MimeType: "video/h264", SDPFmtpLine: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"}, "2", "2")
        if err != nil {
            t.Errorf("Failed to create video track")
        }
        if _, err = apc.AddTrack(video2); err != nil {
            t.Errorf("Failed to add video track")
        }
        audio1, err := NewTrackLocalStaticSample(RTPCodecCapability{MimeType: "audio/opus"}, "3", "3")
        if err != nil {
            t.Errorf("Failed to create audio track")
        }
        if _, err = apc.AddTrack(audio1); err != nil {
            t.Errorf("Failed to add audio track")
        }
        audio2, err := NewTrackLocalStaticSample(RTPCodecCapability{MimeType: "audio/opus"}, "4", "4")
        if err != nil {
            t.Errorf("Failed to create audio track")
        }
        if _, err = apc.AddTrack(audio2); err != nil {
            t.Errorf("Failed to add audio track")
        }

        if err = apc.set_remote_description(offer); err != nil {
            t.Errorf("set_remote_description failed: %s", err)
        }

        answer, err := apc.create_answer(nil)
        if err != nil {
            t.Errorf("Plan B create_answer failed: %s", err)
        }

        mdNames = getMdNames(answer.parsed)
        assert.ObjectsAreEqual(mdNames, []string{"video", "audio", "data"})

        // Verify that each section has 2 SSRCs (one for each sender)
        for _, section := range []string{"video", "audio"} {
            for _, media := range answer.parsed.MediaDescriptions {
                if media.MediaName.Media == section {
                    assert.Lenf(t, extractSsrcList(media), 2, "%q should have 2 SSRCs in Plan-B mode", section)
                }
            }
        }

        closePairNow(t, apc, opc)
    }

    func TestSDPSemantics_UnifiedPlanWithFallback(t *testing.T) {
        report := test.CheckRoutines(t)
        defer report()

        lim := test.TimeOut(time.Second * 30)
        defer lim.Stop()

        opc, err := new_peer_connection(Configuration{
            SDPSemantics: SDPSemanticsPlanB,
        })
        if err != nil {
            t.Errorf("new_peer_connection failed: %v", err)
        }

        if _, err = opc.AddTransceiverFromKind(RTPCodecTypeVideo, RTPTransceiverInit{
            Direction: RTPTransceiverDirectionRecvonly,
        }); err != nil {
            t.Errorf("Failed to add transceiver")
        }
        if _, err = opc.AddTransceiverFromKind(RTPCodecTypeAudio, RTPTransceiverInit{
            Direction: RTPTransceiverDirectionRecvonly,
        }); err != nil {
            t.Errorf("Failed to add transceiver")
        }

        offer, err := opc.CreateOffer(nil)
        if err != nil {
            t.Errorf("Plan B CreateOffer failed: %s", err)
        }

        mdNames := getMdNames(offer.parsed)
        assert.ObjectsAreEqual(mdNames, []string{"video", "audio", "data"})

        apc, err := new_peer_connection(Configuration{
            SDPSemantics: SDPSemanticsUnifiedPlanWithFallback,
        })
        if err != nil {
            t.Errorf("new_peer_connection failed: %v", err)
        }

        video1, err := NewTrackLocalStaticSample(RTPCodecCapability{MimeType: "video/h264", SDPFmtpLine: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"}, "1", "1")
        if err != nil {
            t.Errorf("Failed to create video track")
        }
        if _, err = apc.AddTrack(video1); err != nil {
            t.Errorf("Failed to add video track")
        }
        video2, err := NewTrackLocalStaticSample(RTPCodecCapability{MimeType: "video/h264", SDPFmtpLine: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"}, "2", "2")
        if err != nil {
            t.Errorf("Failed to create video track")
        }
        if _, err = apc.AddTrack(video2); err != nil {
            t.Errorf("Failed to add video track")
        }
        audio1, err := NewTrackLocalStaticSample(RTPCodecCapability{MimeType: "audio/opus"}, "3", "3")
        if err != nil {
            t.Errorf("Failed to create audio track")
        }
        if _, err = apc.AddTrack(audio1); err != nil {
            t.Errorf("Failed to add audio track")
        }
        audio2, err := NewTrackLocalStaticSample(RTPCodecCapability{MimeType: "audio/opus"}, "4", "4")
        if err != nil {
            t.Errorf("Failed to create audio track")
        }
        if _, err = apc.AddTrack(audio2); err != nil {
            t.Errorf("Failed to add audio track")
        }

        if err = apc.set_remote_description(offer); err != nil {
            t.Errorf("set_remote_description failed: %s", err)
        }

        answer, err := apc.create_answer(nil)
        if err != nil {
            t.Errorf("Plan B create_answer failed: %s", err)
        }

        mdNames = getMdNames(answer.parsed)
        assert.ObjectsAreEqual(mdNames, []string{"video", "audio", "data"})

        extractSsrcList := func(md *sdp.MediaDescription) []string {
            ssrcMap := map[string]struct{}{}
            for _, attr := range md.Attributes {
                if attr.Key == ssrcStr {
                    ssrc := strings.Fields(attr.Value)[0]
                    ssrcMap[ssrc] = struct{}{}
                }
            }
            ssrcList := make([]string, 0, len(ssrcMap))
            for ssrc := range ssrcMap {
                ssrcList = append(ssrcList, ssrc)
            }
            return ssrcList
        }
        // Verify that each section has 2 SSRCs (one for each sender)
        for _, section := range []string{"video", "audio"} {
            for _, media := range answer.parsed.MediaDescriptions {
                if media.MediaName.Media == section {
                    assert.Lenf(t, extractSsrcList(media), 2, "%q should have 2 SSRCs in Plan-B fallback mode", section)
                }
            }
        }

        closePairNow(t, apc, opc)
    }
     */
}
