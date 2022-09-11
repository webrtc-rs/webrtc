#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A bare value or constraint specifying a range of accepted values.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `BareOrValueConstraint<T>` type aims to be a generalization over
/// multiple types in the spec.
///
/// | Rust                               | W3C                                   |
/// | ---------------------------------- | ------------------------------------- |
/// | `BareOrValueRangeConstraint<u64>` | [`ConstrainULong`][constrain_ulong]   |
/// | `BareOrValueRangeConstraint<f64>` | [`ConstrainDouble`][constrain_double] |
///
/// [constrain_double]: https://www.w3.org/TR/mediacapture-streams/#dom-constraindouble
/// [constrain_ulong]: https://www.w3.org/TR/mediacapture-streams/#dom-constrainulong
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum BareOrValueRangeConstraint<T> {
    Bare(T),
    Constraint(ValueRangeConstraint<T>),
}

impl<T> Default for BareOrValueRangeConstraint<T> {
    fn default() -> Self {
        Self::Constraint(Default::default())
    }
}

impl<T> From<T> for BareOrValueRangeConstraint<T> {
    fn from(bare: T) -> Self {
        Self::Bare(bare)
    }
}

impl<T> From<ValueRangeConstraint<T>> for BareOrValueRangeConstraint<T> {
    fn from(constraint: ValueRangeConstraint<T>) -> Self {
        Self::Constraint(constraint)
    }
}

/// A constraint specifying a range of accepted values.
///
/// Corresponding W3C spec types as per ["Media Capture and Streams"][spec]:
/// - `ConstrainDouble` => `ValueRangeConstraint<f64>`
/// - `ConstrainULong` => `ValueRangeConstraint<u64>`
///
/// [spec]: https://www.w3.org/TR/mediacapture-streams
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ValueRangeConstraint<T> {
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
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub exact: Option<T>,
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub ideal: Option<T>,
}

impl<T> ValueRangeConstraint<T> {
    pub fn exact_only(exact: T) -> Self {
        Self {
            min: None,
            max: None,
            exact: Some(exact),
            ideal: None,
        }
    }

    pub fn ideal_only(ideal: T) -> Self {
        Self {
            min: None,
            max: None,
            exact: None,
            ideal: Some(ideal),
        }
    }

    pub fn is_required(&self) -> bool {
        self.min.is_some() || self.max.is_some() || self.exact.is_some()
    }
}

impl<T> Default for ValueRangeConstraint<T> {
    fn default() -> Self {
        Self {
            min: None,
            max: None,
            exact: None,
            ideal: None,
        }
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    macro_rules! test_serde {
        ($t:ty => {
            value: $value:expr
        }) => {
            type Subject = BareOrValueRangeConstraint<$t>;

            #[test]
            fn default() {
                let subject = Subject::default();
                let json = serde_json::json!({});

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn bare() {
                let subject = Subject::Bare($value.to_owned());
                let json = serde_json::json!($value);

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn exact() {
                let subject = Subject::Constraint(ValueRangeConstraint::exact_only($value.to_owned()));
                let json = serde_json::json!({
                    "exact": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn ideal() {
                let subject = Subject::Constraint(ValueRangeConstraint::ideal_only($value.to_owned()));
                let json = serde_json::json!({
                    "ideal": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }
        };
    }

    mod f64 {
        use super::*;

        test_serde!(f64 => {
            value: 42.0
        });
    }

    mod u64 {
        use super::*;

        test_serde!(u64 => {
            value: 42
        });
    }
}
