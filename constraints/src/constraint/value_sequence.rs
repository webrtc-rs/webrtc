#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackConstraintResolutionStrategy;

/// A bare value or constraint specifying a sequence of accepted values.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `ValueConstraint<T>` type aims to be a generalization over
/// multiple types in the spec.
///
/// | Rust                                     | W3C                                          |
/// | ---------------------------------------- | -------------------------------------------- |
/// | `ValueSequenceConstraint<String>` | [`ConstrainDOMString`][constrain_dom_string] |
///
/// [constrain_dom_string]: https://www.w3.org/TR/mediacapture-streams/#dom-constraindomstring
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum ValueSequenceConstraint<T> {
    /// A bare-valued media track constraint.
    Bare(Vec<T>),
    /// A fully-qualified media track constraint.
    Constraint(ResolvedValueSequenceConstraint<T>),
}

impl<T> Default for ValueSequenceConstraint<T> {
    fn default() -> Self {
        Self::Constraint(Default::default())
    }
}

impl<T> From<T> for ValueSequenceConstraint<T> {
    fn from(bare: T) -> Self {
        Self::Bare(vec![bare])
    }
}

impl<T> From<Vec<T>> for ValueSequenceConstraint<T> {
    fn from(bare: Vec<T>) -> Self {
        Self::Bare(bare)
    }
}

impl<T> From<ResolvedValueSequenceConstraint<T>> for ValueSequenceConstraint<T> {
    fn from(constraint: ResolvedValueSequenceConstraint<T>) -> Self {
        Self::Constraint(constraint)
    }
}

impl<T> ValueSequenceConstraint<T>
where
    T: Clone,
{
    /// Returns a resolved representation of the constraint
    /// with bare values resolved to fully-qualified constraints.
    pub fn to_resolved(
        &self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ResolvedValueSequenceConstraint<T> {
        self.clone().into_resolved(strategy)
    }

    /// Consumes the constraint, returning a resolved representation of the
    /// constraint with bare values resolved to fully-qualified constraints.
    pub fn into_resolved(
        self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ResolvedValueSequenceConstraint<T> {
        match self {
            Self::Bare(bare) => match strategy {
                MediaTrackConstraintResolutionStrategy::BareToIdeal => {
                    ResolvedValueSequenceConstraint::default().ideal(bare)
                }
                MediaTrackConstraintResolutionStrategy::BareToExact => {
                    ResolvedValueSequenceConstraint::default().exact(bare)
                }
            },
            Self::Constraint(constraint) => constraint,
        }
    }
}

impl<T> ValueSequenceConstraint<T> {
    /// Returns `true` if `self` is empty, otherwise `false`.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Bare(bare) => bare.is_empty(),
            Self::Constraint(constraint) => constraint.is_empty(),
        }
    }
}

/// A constraint specifying a sequence of accepted values.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `ValueSequenceConstraint<T>` type aims to be a
/// generalization over multiple types in the W3C spec:
///
/// | Rust                              | W3C                                                               |
/// | --------------------------------- | ----------------------------------------------------------------- |
/// | `ResolvedValueSequenceConstraint<String>` | [`ConstrainDOMStringParameters`][constrain_dom_string_parameters] |
///
/// [constrain_dom_string_parameters]: https://www.w3.org/TR/mediacapture-streams/#dom-constraindomstringparameters
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ResolvedValueSequenceConstraint<T> {
    /// The exact required value for this property.
    ///
    /// This is a required value.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub exact: Option<Vec<T>>,
    /// The ideal (target) value for this property.
    ///
    /// This is an optional value.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub ideal: Option<Vec<T>>,
}

impl<T> ResolvedValueSequenceConstraint<T> {
    /// Consumes `self`, returning a corresponding constraint
    /// with the exact required value set to `exact`.
    #[inline]
    pub fn exact<U>(mut self, exact: U) -> Self
    where
        Option<Vec<T>>: From<U>,
    {
        self.exact = exact.into();
        self
    }

    /// Consumes `self`, returning a corresponding constraint
    /// with the ideal required value set to `ideal`.
    #[inline]
    pub fn ideal<U>(mut self, ideal: U) -> Self
    where
        Option<Vec<T>>: From<U>,
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
        let exact_is_empty = self.exact.as_ref().map_or(true, Vec::is_empty);
        let ideal_is_empty = self.ideal.as_ref().map_or(true, Vec::is_empty);
        exact_is_empty && ideal_is_empty
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

impl<T> Default for ResolvedValueSequenceConstraint<T> {
    fn default() -> Self {
        Self {
            exact: None,
            ideal: None,
        }
    }
}

impl<T> std::fmt::Display for ResolvedValueSequenceConstraint<T>
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
            (ResolvedValueSequenceConstraint::default(), "(<empty>)"),
            (
                ResolvedValueSequenceConstraint::default().exact(vec![1, 2]),
                "(x == [1, 2])",
            ),
            (
                ResolvedValueSequenceConstraint::default().ideal(vec![2, 3]),
                "(x ~= [2, 3])",
            ),
            (
                ResolvedValueSequenceConstraint::default()
                    .exact(vec![1, 2])
                    .ideal(vec![2, 3]),
                "(x == [1, 2] && x ~= [2, 3])",
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
            (ResolvedValueSequenceConstraint::default(), false),
            (
                ResolvedValueSequenceConstraint::default().exact(vec![true]),
                true,
            ),
            (
                ResolvedValueSequenceConstraint::default().ideal(vec![true]),
                false,
            ),
            (
                ResolvedValueSequenceConstraint::default()
                    .exact(vec![true])
                    .ideal(vec![true]),
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
            let constraint = ValueSequenceConstraint::Bare(vec![true]);

            assert!(!constraint.is_empty());
        }

        #[test]
        fn constraint() {
            let scenarios = [
                (ResolvedValueSequenceConstraint::default(), true),
                (
                    ResolvedValueSequenceConstraint::default().exact(vec![true]),
                    false,
                ),
                (
                    ResolvedValueSequenceConstraint::default().ideal(vec![true]),
                    false,
                ),
                (
                    ResolvedValueSequenceConstraint::default()
                        .exact(vec![true])
                        .ideal(vec![true]),
                    false,
                ),
            ];

            for (constraint, expected) in scenarios {
                let constraint = ValueSequenceConstraint::<bool>::Constraint(constraint);

                let actual = constraint.is_empty();

                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn resolve_to_advanced() {
        let constraints = [
            ValueSequenceConstraint::Bare(vec![true]),
            ValueSequenceConstraint::Constraint(
                ResolvedValueSequenceConstraint::default().exact(vec![true]),
            ),
        ];
        let strategy = MediaTrackConstraintResolutionStrategy::BareToExact;

        for constraint in constraints {
            let actuals = [
                constraint.to_resolved(strategy),
                constraint.into_resolved(strategy),
            ];

            let expected = ResolvedValueSequenceConstraint::default().exact(vec![true]);

            for actual in actuals {
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn resolve_to_basic() {
        let constraints = [
            ValueSequenceConstraint::Bare(vec![true]),
            ValueSequenceConstraint::Constraint(
                ResolvedValueSequenceConstraint::default().ideal(vec![true]),
            ),
        ];
        let strategy = MediaTrackConstraintResolutionStrategy::BareToIdeal;

        for constraint in constraints {
            let actuals = [
                constraint.to_resolved(strategy),
                constraint.into_resolved(strategy),
            ];

            let expected = ResolvedValueSequenceConstraint::default().ideal(vec![true]);

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
            values: [$($values:expr),*]
        }) => {
            type Subject = ValueSequenceConstraint<$t>;

            #[test]
            fn default() {
                let subject = Subject::default();
                let json = serde_json::json!({});

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn bare() {
                let subject = Subject::Bare(vec![$($values.to_owned()),*].into());
                let json = serde_json::json!([$($values),*]);

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn exact_constraint() {
                let subject = Subject::Constraint(ResolvedValueSequenceConstraint::default().exact(vec![$($values.to_owned()),*]));
                let json = serde_json::json!({
                    "exact": [$($values),*],
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn ideal_constraint() {
                let subject = Subject::Constraint(ResolvedValueSequenceConstraint::default().ideal(vec![$($values.to_owned()),*]));
                let json = serde_json::json!({
                    "ideal": [$($values),*],
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn full_constraint() {
                let subject = Subject::Constraint(ResolvedValueSequenceConstraint::default().exact(vec![$($values.to_owned()),*]).ideal(vec![$($values.to_owned()),*]));
                let json = serde_json::json!({
                    "exact": [$($values),*],
                    "ideal": [$($values),*],
                });

                test_serde_symmetry!(subject: subject, json: json);
            }
        };
    }

    mod string {
        use super::*;

        test_serde!(String => {
            values: ["VALUE_0", "VALUE_1"]
        });
    }
}
