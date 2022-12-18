#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A capability specifying a single supported value.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `MediaTrackValueCapability<T>` type aims to be a
/// generalization over multiple types in the W3C spec:
///
/// | Rust                                | W3C                       |
/// | ----------------------------------- | ------------------------- |
/// | `MediaTrackValueCapability<String>` | [`DOMString`][dom_string] |
///
/// [dom_string]: https://webidl.spec.whatwg.org/#idl-DOMString
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct MediaTrackValueCapability<T> {
    pub value: T,
}

impl<T> From<T> for MediaTrackValueCapability<T> {
    fn from(value: T) -> Self {
        Self { value }
    }
}

impl From<&str> for MediaTrackValueCapability<String> {
    fn from(value: &str) -> Self {
        Self {
            value: value.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Subject = MediaTrackValueCapability<String>;

    #[test]
    fn from_str() {
        let subject = Subject::from("string");

        let actual = subject.value.as_str();
        let expected = "string";

        assert_eq!(actual, expected);
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    type Subject = MediaTrackValueCapability<String>;

    #[test]
    fn customized() {
        let subject = Subject {
            value: "string".to_owned(),
        };
        let json = serde_json::json!("string");

        test_serde_symmetry!(subject: subject, json: json);
    }
}
