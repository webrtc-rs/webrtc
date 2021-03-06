use super::sdp_type::SDPType;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// SessionDescription is used to expose local and remote session descriptions.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionDescription {
    pub sdp_type: SDPType,
    pub sdp: String,
    // This will never be initialized by callers, internal use only
    //parsed *sdp.SessionDescription
}

/// Unmarshal is a helper to deserialize the sdp
impl SessionDescription {
    pub fn unmarshal(&self) -> Result<sdp::session_description::SessionDescription> {
        let mut reader = Cursor::new(self.sdp.as_bytes());
        let parsed = sdp::session_description::SessionDescription::unmarshal(&mut reader)?;
        Ok(parsed)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_session_description_json() {
        let tests = vec![
            (
                SessionDescription {
                    sdp_type: SDPType::Offer,
                    sdp: "sdp".to_owned(),
                },
                r#"{"sdp_type":"Offer","sdp":"sdp"}"#,
            ),
            (
                SessionDescription {
                    sdp_type: SDPType::Pranswer,
                    sdp: "sdp".to_owned(),
                },
                r#"{"sdp_type":"Pranswer","sdp":"sdp"}"#,
            ),
            (
                SessionDescription {
                    sdp_type: SDPType::Answer,
                    sdp: "sdp".to_owned(),
                },
                r#"{"sdp_type":"Answer","sdp":"sdp"}"#,
            ),
            (
                SessionDescription {
                    sdp_type: SDPType::Rollback,
                    sdp: "sdp".to_owned(),
                },
                r#"{"sdp_type":"Rollback","sdp":"sdp"}"#,
            ),
            (
                SessionDescription {
                    sdp_type: SDPType::Unspecified,
                    sdp: "sdp".to_owned(),
                },
                r#"{"sdp_type":"Unspecified","sdp":"sdp"}"#,
            ),
        ];

        for (desc, expected_string) in tests {
            let result = serde_json::to_string(&desc);
            assert!(result.is_ok(), "testCase: marshal err: {:?}", result);
            let desc_data = result.unwrap();
            assert_eq!(desc_data, expected_string, "string is not expected");

            let result = serde_json::from_str::<SessionDescription>(&desc_data);
            assert!(result.is_ok(), "testCase: unmarshal err: {:?}", result);
            assert_eq!(result.unwrap(), desc);
        }
    }

    #[test]
    fn test_session_description_unmarshal() {
        /*TODO: pc, err := NewPeerConnection(Configuration{})
        assert.NoError(t, err)
        offer, err := pc.CreateOffer(nil)
        assert.NoError(t, err)
        desc := SessionDescription{
            Type: offer.Type,
            SDP:  offer.SDP,
        }
        assert.Nil(t, desc.parsed)
        parsed1, err := desc.Unmarshal()
        assert.NotNil(t, parsed1)
        assert.NotNil(t, desc.parsed)
        assert.NoError(t, err)
        parsed2, err2 := desc.Unmarshal()
        assert.NotNil(t, parsed2)
        assert.NoError(t, err2)
        assert.NoError(t, pc.Close())

        // check if the two parsed results _really_ match, could be affected by internal caching
        assert.True(t, reflect.DeepEqual(parsed1, parsed2))
         */
    }
}
