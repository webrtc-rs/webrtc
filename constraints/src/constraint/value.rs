#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackConstraintResolutionStrategy;

/// A bare value or constraint specifying a single accepted value.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `BareOrValueConstraint<T>` type aims to be a generalization over
/// multiple types in the spec.
///
/// | Rust                           | W3C                                     |
/// | ------------------------------ | --------------------------------------- |
/// | `BareOrValueConstraint<bool>` | [`ConstrainBoolean`][constrain_boolean] |
///
/// [constrain_boolean]: https://www.w3.org/TR/mediacapture-streams/#dom-constrainboolean
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum BareOrValueConstraint<T> {
    Bare(T),
    Constraint(ValueConstraint<T>),
}

impl<T> Default for BareOrValueConstraint<T> {
    fn default() -> Self {
        Self::Constraint(Default::default())
    }
}

impl<T> From<T> for BareOrValueConstraint<T> {
    fn from(bare: T) -> Self {
        Self::Bare(bare)
    }
}

impl<T> From<ValueConstraint<T>> for BareOrValueConstraint<T> {
    fn from(constraint: ValueConstraint<T>) -> Self {
        Self::Constraint(constraint)
    }
}

impl<T> BareOrValueConstraint<T>
where
    T: Clone,
{
    pub fn to_resolved(
        &self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ValueConstraint<T> {
        self.clone().into_resolved(strategy)
    }

    pub fn into_resolved(
        self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ValueConstraint<T> {
        match self {
            Self::Bare(bare) => match strategy {
                MediaTrackConstraintResolutionStrategy::BareToIdeal => {
                    ValueConstraint::ideal_only(bare)
                }
                MediaTrackConstraintResolutionStrategy::BareToExact => {
                    ValueConstraint::exact_only(bare)
                }
            },
            Self::Constraint(constraint) => constraint,
        }
    }
}

impl<T> BareOrValueConstraint<T> {
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
/// since the `BareOrValueConstraint<T>` type aims to be a
/// generalization over multiple types in the W3C spec:
///
/// | Rust                           | W3C                                     |
/// | ------------------------------ | --------------------------------------- |
/// | `ValueConstraint<bool>` | [`ConstrainBooleanParameters`][constrain_boolean_parameters] |
///
/// [constrain_boolean_parameters]: https://www.w3.org/TR/mediacapture-streams/#dom-constrainbooleanparameters
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ValueConstraint<T> {
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

impl<T> ValueConstraint<T> {
    pub fn exact_only(exact: T) -> Self {
        Self {
            exact: Some(exact),
            ideal: None,
        }
    }

    pub fn ideal_only(ideal: T) -> Self {
        Self {
            exact: None,
            ideal: Some(ideal),
        }
    }

    pub fn is_required(&self) -> bool {
        self.exact.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.exact.is_none() && self.ideal.is_none()
    }
}

impl<T> Default for ValueConstraint<T> {
    fn default() -> Self {
        Self {
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
        let constraint = BareOrValueConstraint::Bare(true);
        let strategy = MediaTrackConstraintResolutionStrategy::BareToExact;
        let actual: ValueConstraint<bool> = constraint.into_resolved(strategy);
        let expected = ValueConstraint::exact_only(true);

        assert_eq!(actual, expected);
    }

    #[test]
    fn resolve_to_basic() {
        let constraint = BareOrValueConstraint::Bare(true);
        let strategy = MediaTrackConstraintResolutionStrategy::BareToIdeal;
        let actual: ValueConstraint<bool> = constraint.into_resolved(strategy);
        let expected = ValueConstraint::ideal_only(true);

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
            type Subject = BareOrValueConstraint<$t>;

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
                let subject = Subject::Constraint(ValueConstraint::exact_only($value.to_owned()));
                let json = serde_json::json!({
                    "exact": $value,
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn ideal() {
                let subject = Subject::Constraint(ValueConstraint::ideal_only($value.to_owned()));
                let json = serde_json::json!({
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
