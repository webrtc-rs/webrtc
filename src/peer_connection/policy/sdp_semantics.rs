use serde::{Deserialize, Serialize};
use std::fmt;

/// SDPSemantics determines which style of SDP offers and answers
/// can be used
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum RTCSdpSemantics {
    Unspecified = 0,

    /// UnifiedPlan uses unified-plan offers and answers
    /// (the default in Chrome since M72)
    /// https://tools.ietf.org/html/draft-roach-mmusic-unified-plan-00
    #[serde(rename = "unified-plan")]
    UnifiedPlan = 1,

    /// PlanB uses plan-b offers and answers
    /// NB: This format should be considered deprecated
    /// https://tools.ietf.org/html/draft-uberti-rtcweb-plan-00
    #[serde(rename = "plan-b")]
    PlanB = 2,

    /// UnifiedPlanWithFallback prefers unified-plan
    /// offers and answers, but will respond to a plan-b offer
    /// with a plan-b answer
    #[serde(rename = "unified-plan-with-fallback")]
    UnifiedPlanWithFallback = 3,
}

impl Default for RTCSdpSemantics {
    fn default() -> Self {
        RTCSdpSemantics::UnifiedPlan
    }
}

const SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK: &str = "unified-plan-with-fallback";
const SDP_SEMANTICS_UNIFIED_PLAN: &str = "unified-plan";
const SDP_SEMANTICS_PLAN_B: &str = "plan-b";

impl From<&str> for RTCSdpSemantics {
    fn from(raw: &str) -> Self {
        match raw {
            SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK => RTCSdpSemantics::UnifiedPlanWithFallback,
            SDP_SEMANTICS_UNIFIED_PLAN => RTCSdpSemantics::UnifiedPlan,
            SDP_SEMANTICS_PLAN_B => RTCSdpSemantics::PlanB,
            _ => RTCSdpSemantics::Unspecified,
        }
    }
}

impl fmt::Display for RTCSdpSemantics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTCSdpSemantics::UnifiedPlanWithFallback => SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK,
            RTCSdpSemantics::UnifiedPlan => SDP_SEMANTICS_UNIFIED_PLAN,
            RTCSdpSemantics::PlanB => SDP_SEMANTICS_PLAN_B,
            RTCSdpSemantics::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::api::media_engine::MediaEngine;
    use crate::api::APIBuilder;
    use crate::error::Result;
    use crate::peer_connection::configuration::RTCConfiguration;
    use crate::peer_connection::peer_connection_test::close_pair_now;
    use crate::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
    use crate::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
    use crate::rtp_transceiver::RTCRtpTransceiverInit;
    use crate::track::track_local::track_local_static_sample::TrackLocalStaticSample;
    use crate::track::track_local::TrackLocal;
    use crate::SSRC_STR;

    use sdp::media_description::MediaDescription;
    use sdp::session_description::SessionDescription;
    use std::collections::HashSet;
    use std::sync::Arc;

    #[test]
    fn test_sdp_semantics_string() {
        let tests = vec![
            (RTCSdpSemantics::Unspecified, "Unspecified"),
            (
                RTCSdpSemantics::UnifiedPlanWithFallback,
                "unified-plan-with-fallback",
            ),
            (RTCSdpSemantics::PlanB, "plan-b"),
            (RTCSdpSemantics::UnifiedPlan, "unified-plan"),
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

    #[tokio::test]
    async fn test_sdp_semantics_plan_b_offer_transceivers() -> Result<()> {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        let api = APIBuilder::new().with_media_engine(m).build();

        let opc = api
            .new_peer_connection(RTCConfiguration {
                sdp_semantics: RTCSdpSemantics::PlanB,
                ..Default::default()
            })
            .await?;

        opc.add_transceiver_from_kind(
            RTPCodecType::Video,
            &[RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Sendrecv,
                send_encodings: vec![],
            }],
        )
        .await?;

        opc.add_transceiver_from_kind(
            RTPCodecType::Video,
            &[RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Sendrecv,
                send_encodings: vec![],
            }],
        )
        .await?;

        opc.add_transceiver_from_kind(
            RTPCodecType::Audio,
            &[RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Sendrecv,
                send_encodings: vec![],
            }],
        )
        .await?;

        opc.add_transceiver_from_kind(
            RTPCodecType::Audio,
            &[RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Sendrecv,
                send_encodings: vec![],
            }],
        )
        .await?;

        let offer = opc.create_offer(None).await?;

        if let Some(parsed) = &offer.parsed {
            let md_names = get_md_names(parsed);
            assert_eq!(md_names, &["video".to_owned(), "audio".to_owned()]);

            // Verify that each section has 2 SSRCs (one for each transceiver)
            for section in &["video".to_owned(), "audio".to_owned()] {
                for media in &parsed.media_descriptions {
                    if &media.media_name.media == section {
                        assert_eq!(extract_ssrc_list(media).len(), 2);
                    }
                }
            }
        }

        let apc = api
            .new_peer_connection(RTCConfiguration {
                sdp_semantics: RTCSdpSemantics::PlanB,
                ..Default::default()
            })
            .await?;

        apc.set_remote_description(offer).await?;

        let answer = apc.create_answer(None).await?;

        if let Some(parsed) = &answer.parsed {
            let md_names = get_md_names(parsed);
            assert_eq!(md_names, &["video".to_owned(), "audio".to_owned()]);
        }

        close_pair_now(&apc, &opc).await;

        Ok(())
    }

    #[tokio::test]
    async fn test_sdp_semantics_plan_b_answer_senders() -> Result<()> {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        let api = APIBuilder::new().with_media_engine(m).build();

        let opc = api
            .new_peer_connection(RTCConfiguration {
                sdp_semantics: RTCSdpSemantics::PlanB,
                ..Default::default()
            })
            .await?;

        opc.add_transceiver_from_kind(
            RTPCodecType::Video,
            &[RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                send_encodings: vec![],
            }],
        )
        .await?;

        opc.add_transceiver_from_kind(
            RTPCodecType::Audio,
            &[RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                send_encodings: vec![],
            }],
        )
        .await?;

        let offer = opc.create_offer(None).await?;

        if let Some(parsed) = &offer.parsed {
            let md_names = get_md_names(parsed);
            assert_eq!(md_names, &["video".to_owned(), "audio".to_owned()]);
        }

        let apc = api
            .new_peer_connection(RTCConfiguration {
                sdp_semantics: RTCSdpSemantics::PlanB,
                ..Default::default()
            })
            .await?;

        let video1: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "video/h264".to_owned(),
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"
                        .to_owned(),
                ..Default::default()
            },
            "1".to_owned(),
            "1".to_owned(),
        ));
        let _ = apc.add_track(video1).await?;

        let video2: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "video/h264".to_owned(),
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"
                        .to_owned(),
                ..Default::default()
            },
            "2".to_owned(),
            "2".to_owned(),
        ));
        let _ = apc.add_track(video2).await?;

        let audio1: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "audio/opus".to_owned(),
                ..Default::default()
            },
            "3".to_owned(),
            "3".to_owned(),
        ));
        let _ = apc.add_track(audio1).await?;

        let audio2: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "audio/opus".to_owned(),
                ..Default::default()
            },
            "4".to_owned(),
            "4".to_owned(),
        ));
        let _ = apc.add_track(audio2).await?;

        apc.set_remote_description(offer).await?;

        let answer = apc.create_answer(None).await?;

        if let Some(parsed) = &answer.parsed {
            let md_names = get_md_names(parsed);
            assert_eq!(md_names, &["video".to_owned(), "audio".to_owned()]);

            // Verify that each section has 2 SSRCs (one for each transceiver)
            for section in &["video".to_owned(), "audio".to_owned()] {
                for media in &parsed.media_descriptions {
                    if &media.media_name.media == section {
                        assert_eq!(extract_ssrc_list(media).len(), 2);
                    }
                }
            }
        }

        close_pair_now(&apc, &opc).await;

        Ok(())
    }

    #[tokio::test]
    async fn test_sdp_semantics_unified_plan_with_fallback() -> Result<()> {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        let api = APIBuilder::new().with_media_engine(m).build();

        let opc = api
            .new_peer_connection(RTCConfiguration {
                sdp_semantics: RTCSdpSemantics::PlanB,
                ..Default::default()
            })
            .await?;

        opc.add_transceiver_from_kind(
            RTPCodecType::Video,
            &[RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                send_encodings: vec![],
            }],
        )
        .await?;

        opc.add_transceiver_from_kind(
            RTPCodecType::Audio,
            &[RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                send_encodings: vec![],
            }],
        )
        .await?;

        let offer = opc.create_offer(None).await?;

        if let Some(parsed) = &offer.parsed {
            let md_names = get_md_names(parsed);
            assert_eq!(md_names, &["video".to_owned(), "audio".to_owned()]);
        }

        let apc = api
            .new_peer_connection(RTCConfiguration {
                sdp_semantics: RTCSdpSemantics::UnifiedPlanWithFallback,
                ..Default::default()
            })
            .await?;

        let video1: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "video/h264".to_owned(),
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"
                        .to_owned(),
                ..Default::default()
            },
            "1".to_owned(),
            "1".to_owned(),
        ));
        let _ = apc.add_track(video1).await?;

        let video2: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "video/h264".to_owned(),
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"
                        .to_owned(),
                ..Default::default()
            },
            "2".to_owned(),
            "2".to_owned(),
        ));
        let _ = apc.add_track(video2).await?;

        let audio1: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "audio/opus".to_owned(),
                ..Default::default()
            },
            "3".to_owned(),
            "3".to_owned(),
        ));
        let _ = apc.add_track(audio1).await?;

        let audio2: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "audio/opus".to_owned(),
                ..Default::default()
            },
            "4".to_owned(),
            "4".to_owned(),
        ));
        let _ = apc.add_track(audio2).await?;

        apc.set_remote_description(offer).await?;

        let answer = apc.create_answer(None).await?;

        if let Some(parsed) = &answer.parsed {
            let md_names = get_md_names(parsed);
            assert_eq!(md_names, &["video".to_owned(), "audio".to_owned()]);

            // Verify that each section has 2 SSRCs (one for each transceiver)
            for section in &["video".to_owned(), "audio".to_owned()] {
                for media in &parsed.media_descriptions {
                    if &media.media_name.media == section {
                        assert_eq!(extract_ssrc_list(media).len(), 2);
                    }
                }
            }
        }

        close_pair_now(&apc, &opc).await;

        Ok(())
    }
}
