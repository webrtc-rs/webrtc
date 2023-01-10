mod value;
mod value_range;
mod value_sequence;

use std::ops::RangeInclusive;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub use self::{
    value::MediaTrackValueCapability, value_range::MediaTrackValueRangeCapability,
    value_sequence::MediaTrackValueSequenceCapability,
};

/// A single [capability][media_track_capabilities] value of a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// There exists no corresponding type in the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_capabilities]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackcapabilities
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum MediaTrackCapability {
    // IMPORTANT:
    // `BoolSequence` must be ordered before `Bool(…)` in order for
    // `serde` to decode the correct variant.
    /// A sequence of boolean-valued media track capabilities.
    BoolSequence(MediaTrackValueSequenceCapability<bool>),
    /// A single boolean-valued media track capability.
    Bool(MediaTrackValueCapability<bool>),
    // `IntegerRange` must be ordered before `FloatRange(…)` in order for
    // `serde` to decode the correct variant.
    /// A range of integer-valued media track capabilities.
    IntegerRange(MediaTrackValueRangeCapability<u64>),
    /// A range of floating-point-valued media track capabilities.
    FloatRange(MediaTrackValueRangeCapability<f64>),
    // IMPORTANT:
    // `StringSequence` must be ordered before `String(…)` in order for
    // `serde` to decode the correct variant.
    /// A sequence of string-valued media track capabilities.
    StringSequence(MediaTrackValueSequenceCapability<String>),
    /// A single string-valued media track capability.
    String(MediaTrackValueCapability<String>),
}

impl From<bool> for MediaTrackCapability {
    fn from(capability: bool) -> Self {
        Self::Bool(capability.into())
    }
}

impl From<Vec<bool>> for MediaTrackCapability {
    fn from(capability: Vec<bool>) -> Self {
        Self::BoolSequence(capability.into())
    }
}

impl From<RangeInclusive<u64>> for MediaTrackCapability {
    fn from(capability: RangeInclusive<u64>) -> Self {
        Self::IntegerRange(capability.into())
    }
}

impl From<RangeInclusive<f64>> for MediaTrackCapability {
    fn from(capability: RangeInclusive<f64>) -> Self {
        Self::FloatRange(capability.into())
    }
}

impl From<String> for MediaTrackCapability {
    fn from(capability: String) -> Self {
        Self::String(capability.into())
    }
}

impl<'a> From<&'a str> for MediaTrackCapability {
    fn from(capability: &'a str) -> Self {
        let capability: String = capability.to_owned();
        Self::from(capability)
    }
}

impl From<Vec<String>> for MediaTrackCapability {
    fn from(capability: Vec<String>) -> Self {
        Self::StringSequence(capability.into())
    }
}

impl From<Vec<&str>> for MediaTrackCapability {
    fn from(capability: Vec<&str>) -> Self {
        let capability: Vec<String> = capability.into_iter().map(|c| c.to_owned()).collect();
        Self::from(capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Subject = MediaTrackCapability;

    mod from {
        use super::*;

        #[test]
        fn bool_sequence() {
            let actual = Subject::from(vec![false, true]);
            let expected = Subject::BoolSequence(vec![false, true].into());

            assert_eq!(actual, expected);
        }

        #[test]
        fn bool() {
            let actual = Subject::from(true);
            let expected = Subject::Bool(true.into());

            assert_eq!(actual, expected);
        }

        #[test]
        fn integer_range() {
            let actual = Subject::from(12..=34);
            let expected = Subject::IntegerRange((12..=34).into());

            assert_eq!(actual, expected);
        }

        #[test]
        fn float() {
            let actual = Subject::from(1.2..=3.4);
            let expected = Subject::FloatRange((1.2..=3.4).into());

            assert_eq!(actual, expected);
        }

        #[test]
        fn string_sequence() {
            let actual = Subject::from(vec!["foo".to_owned(), "bar".to_owned()]);
            let expected = Subject::StringSequence(vec!["foo".to_owned(), "bar".to_owned()].into());

            assert_eq!(actual, expected);
        }

        #[test]
        fn string() {
            let actual = Subject::from("foo".to_owned());
            let expected = Subject::String("foo".to_owned().into());

            assert_eq!(actual, expected);
        }
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    type Subject = MediaTrackCapability;

    #[test]
    fn bool_sequence() {
        let subject = Subject::BoolSequence(vec![false, true].into());
        let json = serde_json::json!([false, true]);

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn bool() {
        let subject = Subject::Bool(true.into());
        let json = serde_json::json!(true);

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn integer_range() {
        let subject = Subject::IntegerRange((12..=34).into());
        let json = serde_json::json!({
            "min": 12,
            "max": 34,
        });

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn float() {
        let subject = Subject::FloatRange((1.2..=3.4).into());
        let json = serde_json::json!({
            "min": 1.2,
            "max": 3.4,
        });

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn string_sequence() {
        let subject = Subject::StringSequence(vec!["foo".to_owned(), "bar".to_owned()].into());
        let json = serde_json::json!(["foo", "bar"]);

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn string() {
        let subject = Subject::String("foo".to_owned().into());
        let json = serde_json::json!("foo");

        test_serde_symmetry!(subject: subject, json: json);
    }
}
