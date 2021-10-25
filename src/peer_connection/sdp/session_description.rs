use crate::error::Result;

use super::sdp_type::RTCSdpType;

use sdp::session_description::SessionDescription;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// SessionDescription is used to expose local and remote session descriptions.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RTCSessionDescription {
    #[serde(rename = "type")]
    pub sdp_type: RTCSdpType,

    pub sdp: String,

    /// This will never be initialized by callers, internal use only
    #[serde(skip)]
    pub(crate) parsed: Option<SessionDescription>,
}

/// Unmarshal is a helper to deserialize the sdp
impl RTCSessionDescription {
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
            assert!(result.is_ok(), "testCase: marshal err: {:?}", result);
            let desc_data = result.unwrap();
            assert_eq!(desc_data, expected_string, "string is not expected");

            let result = serde_json::from_str::<RTCSessionDescription>(&desc_data);
            assert!(result.is_ok(), "testCase: unmarshal err: {:?}", result);
            if let Ok(sd) = result {
                assert!(sd.sdp == desc.sdp && sd.sdp_type == desc.sdp_type);
            }
        }
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
