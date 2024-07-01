use std::io::Cursor;

use sdp::description::session::SessionDescription;
use serde::{Deserialize, Serialize};

use super::sdp_type::RTCSdpType;
use crate::error::Result;

/// SessionDescription is used to expose local and remote session descriptions.
///
/// ## Specifications
///
/// * [MDN]
/// * [W3C]
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Web/API/RTCSessionDescription
/// [W3C]: https://w3c.github.io/webrtc-pc/#rtcsessiondescription-class
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RTCSessionDescription {
    #[serde(rename = "type")]
    pub sdp_type: RTCSdpType,

    pub sdp: String,

    /// This will never be initialized by callers, internal use only
    #[serde(skip)]
    pub(crate) parsed: Option<SessionDescription>,
}

impl RTCSessionDescription {
    /// Given SDP representing an answer, wrap it in an RTCSessionDescription
    /// that can be given to an RTCPeerConnection.
    pub fn answer(sdp: String) -> Result<RTCSessionDescription> {
        let mut desc = RTCSessionDescription {
            sdp,
            sdp_type: RTCSdpType::Answer,
            parsed: None,
        };

        let parsed = desc.unmarshal()?;
        desc.parsed = Some(parsed);

        Ok(desc)
    }

    /// Given SDP representing an offer, wrap it in an RTCSessionDescription
    /// that can be given to an RTCPeerConnection.
    pub fn offer(sdp: String) -> Result<RTCSessionDescription> {
        let mut desc = RTCSessionDescription {
            sdp,
            sdp_type: RTCSdpType::Offer,
            parsed: None,
        };

        let parsed = desc.unmarshal()?;
        desc.parsed = Some(parsed);

        Ok(desc)
    }

    /// Given SDP representing an answer, wrap it in an RTCSessionDescription
    /// that can be given to an RTCPeerConnection. `pranswer` is used when the
    /// answer may not be final, or when updating a previously sent pranswer.
    pub fn pranswer(sdp: String) -> Result<RTCSessionDescription> {
        let mut desc = RTCSessionDescription {
            sdp,
            sdp_type: RTCSdpType::Pranswer,
            parsed: None,
        };

        let parsed = desc.unmarshal()?;
        desc.parsed = Some(parsed);

        Ok(desc)
    }

    /// Unmarshal is a helper to deserialize the sdp
    pub fn unmarshal(&self) -> Result<SessionDescription> {
        let mut reader = Cursor::new(self.sdp.as_bytes());
        let parsed = SessionDescription::unmarshal(&mut reader)?;
        Ok(parsed)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::api::media_engine::MediaEngine;
    use crate::api::APIBuilder;
    use crate::peer_connection::configuration::RTCConfiguration;

    #[test]
    fn test_session_description_json() {
        let tests = vec![
            (
                RTCSessionDescription {
                    sdp_type: RTCSdpType::Offer,
                    sdp: "sdp".to_owned(),
                    parsed: None,
                },
                r#"{"type":"offer","sdp":"sdp"}"#,
            ),
            (
                RTCSessionDescription {
                    sdp_type: RTCSdpType::Pranswer,
                    sdp: "sdp".to_owned(),
                    parsed: None,
                },
                r#"{"type":"pranswer","sdp":"sdp"}"#,
            ),
            (
                RTCSessionDescription {
                    sdp_type: RTCSdpType::Answer,
                    sdp: "sdp".to_owned(),
                    parsed: None,
                },
                r#"{"type":"answer","sdp":"sdp"}"#,
            ),
            (
                RTCSessionDescription {
                    sdp_type: RTCSdpType::Rollback,
                    sdp: "sdp".to_owned(),
                    parsed: None,
                },
                r#"{"type":"rollback","sdp":"sdp"}"#,
            ),
            (
                RTCSessionDescription {
                    sdp_type: RTCSdpType::Unspecified,
                    sdp: "sdp".to_owned(),
                    parsed: None,
                },
                r#"{"type":"Unspecified","sdp":"sdp"}"#,
            ),
        ];

        for (desc, expected_string) in tests {
            let result = serde_json::to_string(&desc);
            assert!(result.is_ok(), "testCase: marshal err: {result:?}");
            let desc_data = result.unwrap();
            assert_eq!(desc_data, expected_string, "string is not expected");

            let result = serde_json::from_str::<RTCSessionDescription>(&desc_data);
            assert!(result.is_ok(), "testCase: unmarshal err: {result:?}");
            if let Ok(sd) = result {
                assert!(sd.sdp == desc.sdp && sd.sdp_type == desc.sdp_type);
            }
        }
    }

    #[tokio::test]
    async fn test_session_description_answer() -> Result<()> {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        let api = APIBuilder::new().with_media_engine(m).build();

        let offer_pc = api.new_peer_connection(RTCConfiguration::default()).await?;
        let answer_pc = api.new_peer_connection(RTCConfiguration::default()).await?;

        let _ = offer_pc.create_data_channel("foo", None).await?;
        let offer = offer_pc.create_offer(None).await?;
        answer_pc.set_remote_description(offer).await?;

        let answer = answer_pc.create_answer(None).await?;

        let desc = RTCSessionDescription::answer(answer.sdp.clone())?;

        assert!(desc.sdp_type == RTCSdpType::Answer);
        assert!(desc.parsed.is_some());

        assert_eq!(answer.unmarshal()?.marshal(), desc.unmarshal()?.marshal());

        Ok(())
    }

    #[tokio::test]
    async fn test_session_description_offer() -> Result<()> {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        let api = APIBuilder::new().with_media_engine(m).build();

        let pc = api.new_peer_connection(RTCConfiguration::default()).await?;
        let offer = pc.create_offer(None).await?;

        let desc = RTCSessionDescription::offer(offer.sdp.clone())?;

        assert!(desc.sdp_type == RTCSdpType::Offer);
        assert!(desc.parsed.is_some());

        assert_eq!(offer.unmarshal()?.marshal(), desc.unmarshal()?.marshal());

        Ok(())
    }

    #[tokio::test]
    async fn test_session_description_pranswer() -> Result<()> {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        let api = APIBuilder::new().with_media_engine(m).build();

        let offer_pc = api.new_peer_connection(RTCConfiguration::default()).await?;
        let answer_pc = api.new_peer_connection(RTCConfiguration::default()).await?;

        let _ = offer_pc.create_data_channel("foo", None).await?;
        let offer = offer_pc.create_offer(None).await?;
        answer_pc.set_remote_description(offer).await?;

        let answer = answer_pc.create_answer(None).await?;

        let desc = RTCSessionDescription::pranswer(answer.sdp)?;

        assert!(desc.sdp_type == RTCSdpType::Pranswer);
        assert!(desc.parsed.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_session_description_unmarshal() -> Result<()> {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        let api = APIBuilder::new().with_media_engine(m).build();

        let pc = api.new_peer_connection(RTCConfiguration::default()).await?;

        let offer = pc.create_offer(None).await?;

        let desc = RTCSessionDescription {
            sdp_type: offer.sdp_type,
            sdp: offer.sdp,
            ..Default::default()
        };

        assert!(desc.parsed.is_none());

        let parsed1 = desc.unmarshal()?;
        let parsed2 = desc.unmarshal()?;

        pc.close().await?;

        // check if the two parsed results _really_ match, could be affected by internal caching
        assert_eq!(parsed1.marshal(), parsed2.marshal());

        Ok(())
    }
}
