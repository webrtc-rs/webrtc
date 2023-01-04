use std::ops::{RangeFrom, RangeInclusive, RangeToInclusive};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A capability specifying a range of supported values.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `MediaTrackValueRangeCapability<T>` type aims to be a
/// generalization over multiple types in the W3C spec:
///
/// | Rust                                  | W3C                           |
/// | ------------------------------------- | ----------------------------- |
/// | `MediaTrackValueRangeCapability<u64>` | [`ULongRange`][ulong_range]   |
/// | `MediaTrackValueRangeCapability<f64>` | [`DoubleRange`][double_range] |
///
/// [double_range]: https://www.w3.org/TR/mediacapture-streams/#dom-doublerange
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
/// [ulong_range]: https://www.w3.org/TR/mediacapture-streams/#dom-ulongrange
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct MediaTrackValueRangeCapability<T> {
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub min: Option<T>,
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub max: Option<T>,
}

impl<T> Default for MediaTrackValueRangeCapability<T> {
    fn default() -> Self {
        Self {
            min: Default::default(),
            max: Default::default(),
        }
    }
}

impl<T> From<RangeInclusive<T>> for MediaTrackValueRangeCapability<T> {
    fn from(range: RangeInclusive<T>) -> Self {
        let (min, max) = range.into_inner();
        Self {
            min: Some(min),
            max: Some(max),
        }
    }
}

impl<T> From<RangeFrom<T>> for MediaTrackValueRangeCapability<T> {
    fn from(range: RangeFrom<T>) -> Self {
        Self {
            min: Some(range.start),
            max: None,
        }
    }
}

impl<T> From<RangeToInclusive<T>> for MediaTrackValueRangeCapability<T> {
    fn from(range: RangeToInclusive<T>) -> Self {
        Self {
            min: None,
            max: Some(range.end),
        }
    }
}

impl<T> MediaTrackValueRangeCapability<T> {
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialOrd,
    {
        // FIXME(regexident): replace with if-let-chain, once stabilized:
        // Tracking issue: https://github.com/rust-lang/rust/issues/53667
        if let Some(ref min) = self.min {
            if min > value {
                return false;
            }
        }
        // FIXME(regexident): replace with if-let-chain, once stabilized:
        // Tracking issue: https://github.com/rust-lang/rust/issues/53667
        if let Some(ref max) = self.max {
            if max < value {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Subject = MediaTrackValueRangeCapability<i64>;

    #[test]
    fn default() {
        let subject = Subject::default();

        assert_eq!(subject.min, None);
        assert_eq!(subject.max, None);
    }

    mod from {
        use super::*;

        #[test]
        fn range_inclusive() {
            let subject = Subject::from(1..=5);

            assert_eq!(subject.min, Some(1));
            assert_eq!(subject.max, Some(5));
        }

        #[test]
        fn range_from() {
            let subject = Subject::from(1..);

            assert_eq!(subject.min, Some(1));
            assert_eq!(subject.max, None);
        }

        #[test]
        fn range_to_inclusive() {
            let subject = Subject::from(..=5);

            assert_eq!(subject.min, None);
            assert_eq!(subject.max, Some(5));
        }
    }

    mod contains {
        use super::*;

        #[test]
        fn default() {
            let subject = Subject::default();

            assert!(subject.contains(&0));
            assert!(subject.contains(&1));
            assert!(subject.contains(&5));
            assert!(subject.contains(&6));
        }

        #[test]
        fn from_range_inclusive() {
            let subject = Subject::from(1..=5);

            assert!(!subject.contains(&0));
            assert!(subject.contains(&1));
            assert!(subject.contains(&5));
            assert!(!subject.contains(&6));
        }

        #[test]
        fn from_range_from() {
            let subject = Subject::from(1..);

            assert!(!subject.contains(&0));
            assert!(subject.contains(&1));
            assert!(subject.contains(&5));
            assert!(subject.contains(&6));
        }

        #[test]
        fn from_range_to_inclusive() {
            let subject = Subject::from(..=5);

            assert!(subject.contains(&0));
            assert!(subject.contains(&1));
            assert!(subject.contains(&5));
            assert!(!subject.contains(&6));
        }
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    type Subject = MediaTrackValueRangeCapability<i64>;

    #[test]
    fn customized() {
        let subject = Subject {
            min: Some(12),
            max: Some(34),
        };
        let json = serde_json::json!({
            "min": 12,
            "max": 34,
        });

        test_serde_symmetry!(subject: subject, json: json);
    }
}
