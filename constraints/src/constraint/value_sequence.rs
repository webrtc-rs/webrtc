#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackConstraintResolutionStrategy;

/// A bare value or constraint specifying a sequence of accepted values.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `BareOrValueConstraint<T>` type aims to be a generalization over
/// multiple types in the spec.
///
/// | Rust                                     | W3C                                          |
/// | ---------------------------------------- | -------------------------------------------- |
/// | `BareOrValueSequenceConstraint<String>` | [`ConstrainDOMString`][constrain_dom_string] |
///
/// [constrain_dom_string]: https://www.w3.org/TR/mediacapture-streams/#dom-constraindomstring
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum BareOrValueSequenceConstraint<T> {
    Bare(Vec<T>),
    Constraint(ValueSequenceConstraint<T>),
}

impl<T> Default for BareOrValueSequenceConstraint<T> {
    fn default() -> Self {
        Self::Constraint(Default::default())
    }
}

impl<T> From<T> for BareOrValueSequenceConstraint<T> {
    fn from(bare: T) -> Self {
        Self::Bare(vec![bare])
    }
}

impl<T> From<Vec<T>> for BareOrValueSequenceConstraint<T> {
    fn from(bare: Vec<T>) -> Self {
        Self::Bare(bare)
    }
}

impl<T> From<ValueSequenceConstraint<T>> for BareOrValueSequenceConstraint<T> {
    fn from(constraint: ValueSequenceConstraint<T>) -> Self {
        Self::Constraint(constraint)
    }
}

impl<T> BareOrValueSequenceConstraint<T>
where
    T: Clone,
{
    pub fn to_resolved(
        &self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ValueSequenceConstraint<T> {
        self.clone().into_resolved(strategy)
    }

    pub fn into_resolved(
        self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ValueSequenceConstraint<T> {
        match self {
            Self::Bare(bare) => match strategy {
                MediaTrackConstraintResolutionStrategy::BareToIdeal => {
                    ValueSequenceConstraint::ideal_only(bare)
                }
                MediaTrackConstraintResolutionStrategy::BareToExact => {
                    ValueSequenceConstraint::exact_only(bare)
                }
            },
            Self::Constraint(constraint) => constraint,
        }
    }
}

/// A constraint specifying a sequence of accepted values.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `BareOrValueSequenceConstraint<T>` type aims to be a
/// generalization over multiple types in the W3C spec:
///
/// | Rust                              | W3C                                                               |
/// | --------------------------------- | ----------------------------------------------------------------- |
/// | `ValueSequenceConstraint<String>` | [`ConstrainDOMStringParameters`][constrain_dom_string_parameters] |
///
/// [constrain_dom_string_parameters]: https://www.w3.org/TR/mediacapture-streams/#dom-constraindomstringparameters
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ValueSequenceConstraint<T> {
    // See https://developer.mozilla.org/en-US/docs/Web/API/MediaTrackConstraints#constraindomstring
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub exact: Option<Vec<T>>,
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "core::option::Option::is_none")
    )]
    pub ideal: Option<Vec<T>>,
}

impl<T> ValueSequenceConstraint<T> {
    pub fn exact_only(exact: Vec<T>) -> Self {
        Self {
            exact: Some(exact),
            ideal: None,
        }
    }

    pub fn ideal_only(ideal: Vec<T>) -> Self {
        Self {
            exact: None,
            ideal: Some(ideal),
        }
    }

    pub fn is_required(&self) -> bool {
        self.exact.is_some()
    }
}

impl<T> Default for ValueSequenceConstraint<T> {
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
        let constraint = BareOrValueSequenceConstraint::Bare(vec![true]);
        let strategy = MediaTrackConstraintResolutionStrategy::BareToExact;
        let actual: ValueSequenceConstraint<bool> = constraint.into_resolved(strategy);
        let expected = ValueSequenceConstraint::exact_only(vec![true]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn resolve_to_basic() {
        let constraint = BareOrValueSequenceConstraint::Bare(vec![true]);
        let strategy = MediaTrackConstraintResolutionStrategy::BareToIdeal;
        let actual: ValueSequenceConstraint<bool> = constraint.into_resolved(strategy);
        let expected = ValueSequenceConstraint::ideal_only(vec![true]);

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
            values: [$($values:expr),*]
        }) => {
            type Subject = BareOrValueSequenceConstraint<$t>;

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
            fn constraint() {
                let subject = Subject::Constraint(ValueSequenceConstraint::<String> {
                    exact: Some(vec![$($values.to_owned()),*].into()),
                    ideal: None,
                });
                let json = serde_json::json!({
                    "exact": [$($values),*],
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
