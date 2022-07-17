#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A single [setting][media_track_settings] value of a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// There exists no corresponding type in the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_settings]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatracksettings
/// [media_track_supported_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatracksupportedconstraints
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum MediaTrackSetting {
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
}

impl From<bool> for MediaTrackSetting {
    fn from(setting: bool) -> Self {
        Self::Bool(setting)
    }
}

impl From<i64> for MediaTrackSetting {
    fn from(setting: i64) -> Self {
        Self::Integer(setting)
    }
}

impl From<f64> for MediaTrackSetting {
    fn from(setting: f64) -> Self {
        Self::Float(setting)
    }
}

impl From<String> for MediaTrackSetting {
    fn from(setting: String) -> Self {
        Self::String(setting)
    }
}

impl<'a> From<&'a str> for MediaTrackSetting {
    fn from(setting: &'a str) -> Self {
        Self::String(setting.to_owned())
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    type Subject = MediaTrackSetting;

    #[test]
    fn bool() {
        let subject = Subject::Bool(true);
        let json = serde_json::json!(true);

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn integer() {
        let subject = Subject::Integer(42);
        let json = serde_json::json!(42);

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn float() {
        let subject = Subject::Float(4.2);
        let json = serde_json::json!(4.2);

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn string() {
        let subject = Subject::String("string".to_owned());
        let json = serde_json::json!("string");

        test_serde_symmetry!(subject: subject, json: json);
    }
}
