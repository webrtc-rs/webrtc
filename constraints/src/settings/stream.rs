#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackSettings;

/// The settings of a [`MediaStream`][media_stream] object.
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
pub(crate) struct MediaStreamSettings {
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub audio: Option<MediaTrackSettings>,
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub video: Option<MediaTrackSettings>,
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    type Subject = MediaStreamSettings;

    #[test]
    fn default() {
        let subject = Subject::default();
        let json = serde_json::json!({});

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn customized() {
        let subject = MediaStreamSettings {
            audio: Some(MediaTrackSettings::default()),
            video: None,
        };
        let json = serde_json::json!({
            "audio": {}
        });

        test_serde_symmetry!(subject: subject, json: json);
    }
}
