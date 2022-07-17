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
#[derive(Debug, Clone, PartialEq)]
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

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    type Subject = MediaTrackValueCapability<i64>;

    #[test]
    fn customized() {
        let subject = Subject { value: 42 };
        let json = serde_json::json!(42);

        test_serde_symmetry!(subject: subject, json: json);
    }
}
