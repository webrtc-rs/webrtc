#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackConstraintResolutionStrategy;

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
#[derive(Debug, Clone, Eq, PartialEq)]
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

impl<T> BareOrValueRangeConstraint<T>
where
    T: Clone,
{
    pub fn to_resolved(
        &self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ValueRangeConstraint<T> {
        self.clone().into_resolved(strategy)
    }

    pub fn into_resolved(
        self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ValueRangeConstraint<T> {
        match self {
            Self::Bare(bare) => match strategy {
                MediaTrackConstraintResolutionStrategy::BareToIdeal => {
                    ValueRangeConstraint::default().ideal(bare)
                }
                MediaTrackConstraintResolutionStrategy::BareToExact => {
                    ValueRangeConstraint::default().exact(bare)
                }
            },
            Self::Constraint(constraint) => constraint,
        }
    }
}

impl<T> BareOrValueRangeConstraint<T> {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Bare(_) => false,
            Self::Constraint(constraint) => constraint.is_empty(),
        }
    }
}

/// A constraint specifying a range of accepted values.
///
/// Corresponding W3C spec types as per ["Media Capture and Streams"][spec]:
/// - `ConstrainDouble` => `ValueRangeConstraint<f64>`
/// - `ConstrainULong` => `ValueRangeConstraint<u64>`
///
/// [spec]: https://www.w3.org/TR/mediacapture-streams
#[derive(Debug, Clone, Eq, PartialEq)]
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
    #[inline]
    pub fn exact<U>(mut self, exact: U) -> Self
    where
        Option<T>: From<U>,
    {
        self.exact = exact.into();
        self
    }

    #[inline]
    pub fn ideal<U>(mut self, ideal: U) -> Self
    where
        Option<T>: From<U>,
    {
        self.ideal = ideal.into();
        self
    }

    #[inline]
    pub fn min<U>(mut self, min: U) -> Self
    where
        Option<T>: From<U>,
    {
        self.min = min.into();
        self
    }

    #[inline]
    pub fn max<U>(mut self, max: U) -> Self
    where
        Option<T>: From<U>,
    {
        self.max = max.into();
        self
    }

    pub fn is_required(&self) -> bool {
        self.min.is_some() || self.max.is_some() || self.exact.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.min.is_none() && self.max.is_none() && self.exact.is_none() && self.ideal.is_none()
    }
}

impl<T> Default for ValueRangeConstraint<T> {
    #[inline]
    fn default() -> Self {
        Self {
            min: None,
            max: None,
            exact: None,
            ideal: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_to_advanced() {
        let constraint = BareOrValueRangeConstraint::Bare(42);
        let strategy = MediaTrackConstraintResolutionStrategy::BareToExact;
        let actual: ValueRangeConstraint<u64> = constraint.into_resolved(strategy);
        let expected = ValueRangeConstraint::default().exact(42);

        assert_eq!(actual, expected);
    }

    #[test]
    fn resolve_to_basic() {
        let constraint = BareOrValueRangeConstraint::Bare(42);
        let strategy = MediaTrackConstraintResolutionStrategy::BareToIdeal;
        let actual: ValueRangeConstraint<u64> = constraint.into_resolved(strategy);
        let expected = ValueRangeConstraint::default().ideal(42);

        assert_eq!(actual, expected);
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
            fn min_constraint() {
                let subject = Subject::Constraint(ValueRangeConstraint::default().min($value.to_owned()));
                let json = serde_json::json!({
                    "min": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn max_constraint() {
                let subject = Subject::Constraint(ValueRangeConstraint::default().max($value.to_owned()));
                let json = serde_json::json!({
                    "max": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn exact_constraint() {
                let subject = Subject::Constraint(ValueRangeConstraint::default().exact($value.to_owned()));
                let json = serde_json::json!({
                    "exact": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn ideal_constraint() {
                let subject = Subject::Constraint(ValueRangeConstraint::default().ideal($value.to_owned()));
                let json = serde_json::json!({
                    "ideal": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn full_constraint() {
                let subject = Subject::Constraint(ValueRangeConstraint::default().min($value.to_owned()).max($value.to_owned()).exact($value.to_owned()).ideal($value.to_owned()));
                let json = serde_json::json!({
                    "min": $value,
                    "max": $value,
                    "exact": $value,
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
