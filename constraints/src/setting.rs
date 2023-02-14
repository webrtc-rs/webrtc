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
    /// A boolean-valued track setting.
    Bool(bool),
    /// An integer-valued track setting.
    Integer(i64),
    /// A floating-point-valued track setting.
    Float(f64),
    /// A string-valued track setting.
    String(String),
}

impl std::fmt::Display for MediaTrackSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(setting) => f.write_fmt(format_args!("{setting:?}")),
            Self::Integer(setting) => f.write_fmt(format_args!("{setting:?}")),
            Self::Float(setting) => f.write_fmt(format_args!("{setting:?}")),
            Self::String(setting) => f.write_fmt(format_args!("{setting:?}")),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    type Subject = MediaTrackSetting;

    mod from {
        use super::*;

        #[test]
        fn bool() {
            let actual = Subject::from(true);
            let expected = Subject::Bool(true);

            assert_eq!(actual, expected);
        }

        #[test]
        fn integer() {
            let actual = Subject::from(42);
            let expected = Subject::Integer(42);

            assert_eq!(actual, expected);
        }

        #[test]
        fn float() {
            let actual = Subject::from(4.2);
            let expected = Subject::Float(4.2);

            assert_eq!(actual, expected);
        }

        #[test]
        fn string() {
            let actual = Subject::from("string".to_owned());
            let expected = Subject::String("string".to_owned());

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn to_string() {
        assert_eq!(Subject::from(true).to_string(), "true");
        assert_eq!(Subject::from(42).to_string(), "42");
        assert_eq!(Subject::from(4.2).to_string(), "4.2");
        assert_eq!(Subject::from("string".to_owned()).to_string(), "\"string\"");
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
