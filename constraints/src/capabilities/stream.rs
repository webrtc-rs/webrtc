#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackCapabilities;

/// The capabilities of a [`MediaStream`][media_stream] object.
///
/// # W3C Spec Compliance
///
/// There exists no corresponding type in the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastream
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams
#[derive(Default, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub(crate) struct MediaStreamCapabilities {
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub audio: Option<MediaTrackCapabilities>,
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub video: Option<MediaTrackCapabilities>,
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    type Subject = MediaStreamCapabilities;

    #[test]
    fn default() {
        let subject = Subject::default();
        let json = serde_json::json!({});

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn customized() {
        let subject = Subject {
            audio: Some(MediaTrackCapabilities::default()),
            video: None,
        };
        let json = serde_json::json!({
            "audio": {}
        });

        test_serde_symmetry!(subject: subject, json: json);
    }
}
