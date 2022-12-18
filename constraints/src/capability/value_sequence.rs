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

#[cfg(test)]
mod tests {
    use super::*;

    type Subject = MediaTrackValueSequenceCapability<String>;

    #[test]
    fn default() {
        let subject = Subject::default();

        let actual = subject.values;

        let expected: Vec<String> = vec![];

        assert_eq!(actual, expected);
    }

    mod from {
        use super::*;

        #[test]
        fn value() {
            let subject = Subject::from("foo".to_owned());

            let actual = subject.values;

            let expected: Vec<String> = vec!["foo".to_owned()];

            assert_eq!(actual, expected);
        }

        #[test]
        fn values() {
            let subject = Subject::from(vec!["foo".to_owned(), "bar".to_owned()]);

            let actual = subject.values;

            let expected: Vec<String> = vec!["foo".to_owned(), "bar".to_owned()];

            assert_eq!(actual, expected);
        }
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    type Subject = MediaTrackValueSequenceCapability<String>;

    #[test]
    fn customized() {
        let subject = Subject {
            values: vec!["foo".to_owned(), "bar".to_owned()],
        };
        let json = serde_json::json!(["foo", "bar"]);

        test_serde_symmetry!(subject: subject, json: json);
    }
}
