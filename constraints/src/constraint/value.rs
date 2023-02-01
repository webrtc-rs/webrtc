#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackConstraintResolutionStrategy;

/// A bare value or constraint specifying a single accepted value.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `ValueConstraint<T>` type aims to be a generalization over
/// multiple types in the spec.
///
/// | Rust                           | W3C                                     |
/// | ------------------------------ | --------------------------------------- |
/// | `ValueConstraint<bool>` | [`ConstrainBoolean`][constrain_boolean] |
///
/// [constrain_boolean]: https://www.w3.org/TR/mediacapture-streams/#dom-constrainboolean
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum ValueConstraint<T> {
    /// A bare-valued media track constraint.
    Bare(T),
    /// A fully-qualified media track constraint.
    Constraint(ResolvedValueConstraint<T>),
}

impl<T> Default for ValueConstraint<T> {
    fn default() -> Self {
        Self::Constraint(Default::default())
    }
}

impl<T> From<T> for ValueConstraint<T> {
    fn from(bare: T) -> Self {
        Self::Bare(bare)
    }
}

impl<T> From<ResolvedValueConstraint<T>> for ValueConstraint<T> {
    fn from(constraint: ResolvedValueConstraint<T>) -> Self {
        Self::Constraint(constraint)
    }
}

impl<T> ValueConstraint<T>
where
    T: Clone,
{
    /// Returns a resolved representation of the constraint
    /// with bare values resolved to fully-qualified constraints.
    pub fn to_resolved(
        &self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ResolvedValueConstraint<T> {
        self.clone().into_resolved(strategy)
    }

    /// Consumes the constraint, returning a resolved representation of the
    /// constraint with bare values resolved to fully-qualified constraints.
    pub fn into_resolved(
        self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ResolvedValueConstraint<T> {
        match self {
            Self::Bare(bare) => match strategy {
                MediaTrackConstraintResolutionStrategy::BareToIdeal => {
                    ResolvedValueConstraint::default().ideal(bare)
                }
                MediaTrackConstraintResolutionStrategy::BareToExact => {
                    ResolvedValueConstraint::default().exact(bare)
                }
            },
            Self::Constraint(constraint) => constraint,
        }
    }
}

impl<T> ValueConstraint<T> {
    /// Returns `true` if `self` is empty, otherwise `false`.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Bare(_) => false,
            Self::Constraint(constraint) => constraint.is_empty(),
        }
    }
}

/// A constraint specifying a single accepted value.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `ValueConstraint<T>` type aims to be a
/// generalization over multiple types in the W3C spec:
///
/// | Rust                           | W3C                                     |
/// | ------------------------------ | --------------------------------------- |
/// | `ResolvedValueConstraint<bool>` | [`ConstrainBooleanParameters`][constrain_boolean_parameters] |
///
/// [constrain_boolean_parameters]: https://www.w3.org/TR/mediacapture-streams/#dom-constrainbooleanparameters
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ResolvedValueConstraint<T> {
    /// The exact required value for this property.
    ///
    /// This is a required value.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub exact: Option<T>,
    /// The ideal (target) value for this property.
    ///
    /// This is an optional value.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub ideal: Option<T>,
}

impl<T> ResolvedValueConstraint<T> {
    /// Consumes `self`, returning a corresponding constraint
    /// with the exact required value set to `exact`.
    #[inline]
    pub fn exact<U>(mut self, exact: U) -> Self
    where
        Option<T>: From<U>,
    {
        self.exact = exact.into();
        self
    }

    /// Consumes `self`, returning a corresponding constraint
    /// with the ideal required value set to `ideal`.
    #[inline]
    pub fn ideal<U>(mut self, ideal: U) -> Self
    where
        Option<T>: From<U>,
    {
        self.ideal = ideal.into();
        self
    }

    /// Returns `true` if `value.is_some()` is `true` for any of its required values,
    /// otherwise `false`.
    pub fn is_required(&self) -> bool {
        self.exact.is_some()
    }

    /// Returns `true` if `value.is_none()` is `true` for all of its values,
    /// otherwise `false`.
    pub fn is_empty(&self) -> bool {
        self.exact.is_none() && self.ideal.is_none()
    }

    /// Returns a corresponding constraint containing only required values.
    pub fn to_required_only(&self) -> Self
    where
        T: Clone,
    {
        self.clone().into_required_only()
    }

    /// Consumes `self, returning a corresponding constraint
    /// containing only required values.
    pub fn into_required_only(self) -> Self {
        Self {
            exact: self.exact,
            ideal: None,
        }
    }
}

impl<T> Default for ResolvedValueConstraint<T> {
    #[inline]
    fn default() -> Self {
        Self {
            exact: None,
            ideal: None,
        }
    }
}

impl<T> std::fmt::Display for ResolvedValueConstraint<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut is_first = true;
        f.write_str("(")?;
        if let Some(ref exact) = &self.exact {
            f.write_fmt(format_args!("x == {exact:?}"))?;
            is_first = false;
        }
        if let Some(ref ideal) = &self.ideal {
            if !is_first {
                f.write_str(" && ")?;
            }
            f.write_fmt(format_args!("x ~= {ideal:?}"))?;
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
            (ResolvedValueConstraint::default(), "(<empty>)"),
            (
                ResolvedValueConstraint::default().exact(true),
                "(x == true)",
            ),
            (
                ResolvedValueConstraint::default().ideal(true),
                "(x ~= true)",
            ),
            (
                ResolvedValueConstraint::default().exact(true).ideal(true),
                "(x == true && x ~= true)",
            ),
        ];

        for (constraint, expected) in scenarios {
            let actual = constraint.to_string();

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn is_required() {
        let scenarios = [
            (ResolvedValueConstraint::default(), false),
            (ResolvedValueConstraint::default().exact(true), true),
            (ResolvedValueConstraint::default().ideal(true), false),
            (
                ResolvedValueConstraint::default().exact(true).ideal(true),
                true,
            ),
        ];

        for (constraint, expected) in scenarios {
            let actual = constraint.is_required();

            assert_eq!(actual, expected);
        }
    }

    mod is_empty {
        use super::*;

        #[test]
        fn bare() {
            let constraint = ValueConstraint::Bare(true);

            assert!(!constraint.is_empty());
        }

        #[test]
        fn constraint() {
            let scenarios = [
                (ResolvedValueConstraint::default(), true),
                (ResolvedValueConstraint::default().exact(true), false),
                (ResolvedValueConstraint::default().ideal(true), false),
                (
                    ResolvedValueConstraint::default().exact(true).ideal(true),
                    false,
                ),
            ];

            for (constraint, expected) in scenarios {
                let constraint = ValueConstraint::<bool>::Constraint(constraint);

                let actual = constraint.is_empty();

                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn resolve_to_advanced() {
        let constraints = [
            ValueConstraint::Bare(true),
            ValueConstraint::Constraint(ResolvedValueConstraint::default().exact(true)),
        ];
        let strategy = MediaTrackConstraintResolutionStrategy::BareToExact;

        for constraint in constraints {
            let actuals = [
                constraint.to_resolved(strategy),
                constraint.into_resolved(strategy),
            ];

            let expected = ResolvedValueConstraint::default().exact(true);

            for actual in actuals {
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn resolve_to_basic() {
        let constraints = [
            ValueConstraint::Bare(true),
            ValueConstraint::Constraint(ResolvedValueConstraint::default().ideal(true)),
        ];
        let strategy = MediaTrackConstraintResolutionStrategy::BareToIdeal;

        for constraint in constraints {
            let actuals = [
                constraint.to_resolved(strategy),
                constraint.into_resolved(strategy),
            ];

            let expected = ResolvedValueConstraint::default().ideal(true);

            for actual in actuals {
                assert_eq!(actual, expected);
            }
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
            type Subject = ValueConstraint<$t>;

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
            fn exact_constraint() {
                let subject = Subject::Constraint(ResolvedValueConstraint::default().exact($value.to_owned()));
                let json = serde_json::json!({
                    "exact": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn ideal_constraint() {
                let subject = Subject::Constraint(ResolvedValueConstraint::default().ideal($value.to_owned()));
                let json = serde_json::json!({
                    "ideal": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn full_constraint() {
                let subject = Subject::Constraint(ResolvedValueConstraint::default().exact($value.to_owned()).ideal($value.to_owned()));
                let json = serde_json::json!({
                    "exact": $value,
                    "ideal": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }
        };
    }

    mod bool {
        use super::*;

        test_serde!(bool => {
            value: true
        });
    }

    mod string {
        use super::*;

        test_serde!(String => {
            value: "VALUE"
        });
    }
}
