#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A capability specifying a range of supported values.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`sequence<T>`][sequence] from the W3C ["WebIDL"][webidl_spec] spec:
///
/// | Rust                                      | W3C                   |
/// | ----------------------------------------- | --------------------- |
/// | `MediaTrackValueSequenceCapability<bool>` | `sequence<boolean>`   |
/// | `MediaTrackValueSequenceCapability<String>`  | `sequence<DOMString>` |
///
/// [sequence]: https://webidl.spec.whatwg.org/#idl-sequence
/// [webidl_spec]: https://webidl.spec.whatwg.org/
#[derive(Default, Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct MediaTrackValueSequenceCapability<T> {
    pub values: Vec<T>,
}

impl<T> From<T> for MediaTrackValueSequenceCapability<T> {
    fn from(value: T) -> Self {
        Self {
            values: vec![value],
        }
    }
}

impl<T> From<Vec<T>> for MediaTrackValueSequenceCapability<T> {
    fn from(values: Vec<T>) -> Self {
        Self { values }
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    type Subject = MediaTrackValueSequenceCapability<i64>;

    #[test]
    fn customized() {
        let subject = Subject {
            values: vec![1, 2, 3],
        };
        let json = serde_json::json!([1, 2, 3]);

        test_serde_symmetry!(subject: subject, json: json);
    }
}
