#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackConstraintResolutionStrategy;

/// A bare value or constraint specifying a range of accepted values.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `ValueConstraint<T>` type aims to be a generalization over
/// multiple types in the spec.
///
/// | Rust                               | W3C                                   |
/// | ---------------------------------- | ------------------------------------- |
/// | `ValueRangeConstraint<u64>` | [`ConstrainULong`][constrain_ulong]   |
/// | `ValueRangeConstraint<f64>` | [`ConstrainDouble`][constrain_double] |
///
/// [constrain_double]: https://www.w3.org/TR/mediacapture-streams/#dom-constraindouble
/// [constrain_ulong]: https://www.w3.org/TR/mediacapture-streams/#dom-constrainulong
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum ValueRangeConstraint<T> {
    Bare(T),
    Constraint(ResolvedValueRangeConstraint<T>),
}

impl<T> Default for ValueRangeConstraint<T> {
    fn default() -> Self {
        Self::Constraint(Default::default())
    }
}

impl<T> From<T> for ValueRangeConstraint<T> {
    fn from(bare: T) -> Self {
        Self::Bare(bare)
    }
}

impl<T> From<ResolvedValueRangeConstraint<T>> for ValueRangeConstraint<T> {
    fn from(constraint: ResolvedValueRangeConstraint<T>) -> Self {
        Self::Constraint(constraint)
    }
}

impl<T> ValueRangeConstraint<T>
where
    T: Clone,
{
    pub fn to_resolved(
        &self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ResolvedValueRangeConstraint<T> {
        self.clone().into_resolved(strategy)
    }

    pub fn into_resolved(
        self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ResolvedValueRangeConstraint<T> {
        match self {
            Self::Bare(bare) => match strategy {
                MediaTrackConstraintResolutionStrategy::BareToIdeal => {
                    ResolvedValueRangeConstraint::default().ideal(bare)
                }
                MediaTrackConstraintResolutionStrategy::BareToExact => {
                    ResolvedValueRangeConstraint::default().exact(bare)
                }
            },
            Self::Constraint(constraint) => constraint,
        }
    }
}

impl<T> ValueRangeConstraint<T> {
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
/// - `ConstrainDouble` => `ResolvedValueRangeConstraint<f64>`
/// - `ConstrainULong` => `ResolvedValueRangeConstraint<u64>`
///
/// [spec]: https://www.w3.org/TR/mediacapture-streams
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ResolvedValueRangeConstraint<T> {
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

impl<T> ResolvedValueRangeConstraint<T> {
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

    pub fn to_required_only(&self) -> Self
    where
        T: Clone,
    {
        self.clone().into_required_only()
    }

    pub fn into_required_only(self) -> Self {
        Self {
            min: self.min,
            max: self.max,
            exact: self.exact,
            ideal: None,
        }
    }
}

impl<T> Default for ResolvedValueRangeConstraint<T> {
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

impl<T> std::fmt::Display for ResolvedValueRangeConstraint<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut is_first = true;
        f.write_str("(")?;
        if let Some(exact) = &self.exact {
            f.write_fmt(format_args!("x == {:?}", exact))?;
            is_first = false;
        } else if let (Some(min), Some(max)) = (&self.min, &self.max) {
            f.write_fmt(format_args!("{:?} <= x <= {:?}", min, max))?;
            is_first = false;
        } else if let Some(min) = &self.min {
            f.write_fmt(format_args!("{:?} <= x", min))?;
            is_first = false;
        } else if let Some(max) = &self.max {
            f.write_fmt(format_args!("x <= {:?}", max))?;
            is_first = false;
        }
        if let Some(ideal) = &self.ideal {
            if !is_first {
                f.write_str(" && ")?;
            }
            f.write_fmt(format_args!("x ~= {:?}", ideal))?;
            is_first = false;
        }
        if is_first {
            f.write_str("<empty>")?;
        }
        f.write_str(")")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_string() {
        let scenarios = [
            (ResolvedValueRangeConstraint::default(), "(<empty>)"),
            (ResolvedValueRangeConstraint::default().exact(1), "(x == 1)"),
            (ResolvedValueRangeConstraint::default().ideal(2), "(x ~= 2)"),
            (
                ResolvedValueRangeConstraint::default().exact(1).ideal(2),
                "(x == 1 && x ~= 2)",
            ),
        ];

        for (constraint, expected) in scenarios {
            let actual = constraint.to_string();

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn is_required() {
        for min in [false, true] {
            for max in [false, true] {
                for exact in [false, true] {
                    for ideal in [false, true] {
                        let constraint = ResolvedValueRangeConstraint::<u64> {
                            min: min.then_some(42),
                            max: max.then_some(42),
                            exact: exact.then_some(42),
                            ideal: ideal.then_some(42),
                        };

                        let actual = constraint.is_required();
                        let expected = min || max || exact;

                        assert_eq!(actual, expected);
                    }
                }
            }
        }
    }

    mod is_empty {
        use super::*;

        #[test]
        fn bare() {
            let constraint = ValueRangeConstraint::Bare(42);

            assert!(!constraint.is_empty());
        }

        #[test]
        fn constraint() {
            for min in [false, true] {
                for max in [false, true] {
                    for exact in [false, true] {
                        for ideal in [false, true] {
                            let constraint = ResolvedValueRangeConstraint::<u64> {
                                min: min.then_some(42),
                                max: max.then_some(42),
                                exact: exact.then_some(42),
                                ideal: ideal.then_some(42),
                            };

                            let actual = constraint.is_empty();
                            let expected = !(min || max || exact || ideal);

                            assert_eq!(actual, expected);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn resolve_to_advanced() {
    let constraints = [
        ValueRangeConstraint::Bare(42),
        ValueRangeConstraint::Constraint(ResolvedValueRangeConstraint::default().exact(42)),
    ];
    let strategy = MediaTrackConstraintResolutionStrategy::BareToExact;

    for constraint in constraints {
        let actuals = [
            constraint.to_resolved(strategy),
            constraint.into_resolved(strategy),
        ];

        let expected = ResolvedValueRangeConstraint::default().exact(42);

        for actual in actuals {
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn resolve_to_basic() {
    let constraints = [
        ValueRangeConstraint::Bare(42),
        ValueRangeConstraint::Constraint(ResolvedValueRangeConstraint::default().ideal(42)),
    ];
    let strategy = MediaTrackConstraintResolutionStrategy::BareToIdeal;

    for constraint in constraints {
        let actuals = [
            constraint.to_resolved(strategy),
            constraint.into_resolved(strategy),
        ];

        let expected = ResolvedValueRangeConstraint::default().ideal(42);

        for actual in actuals {
            assert_eq!(actual, expected);
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
            type Subject = ValueRangeConstraint<$t>;

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
                let subject = Subject::Constraint(ResolvedValueRangeConstraint::default().min($value.to_owned()));
                let json = serde_json::json!({
                    "min": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn max_constraint() {
                let subject = Subject::Constraint(ResolvedValueRangeConstraint::default().max($value.to_owned()));
                let json = serde_json::json!({
                    "max": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn exact_constraint() {
                let subject = Subject::Constraint(ResolvedValueRangeConstraint::default().exact($value.to_owned()));
                let json = serde_json::json!({
                    "exact": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn ideal_constraint() {
                let subject = Subject::Constraint(ResolvedValueRangeConstraint::default().ideal($value.to_owned()));
                let json = serde_json::json!({
                    "ideal": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn full_constraint() {
                let subject = Subject::Constraint(ResolvedValueRangeConstraint::default().min($value.to_owned()).max($value.to_owned()).exact($value.to_owned()).ideal($value.to_owned()));
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
