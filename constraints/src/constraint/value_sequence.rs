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
#[derive(Debug, Clone, Eq, PartialEq)]
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
                    ValueSequenceConstraint::default().ideal(bare)
                }
                MediaTrackConstraintResolutionStrategy::BareToExact => {
                    ValueSequenceConstraint::default().exact(bare)
                }
            },
            Self::Constraint(constraint) => constraint,
        }
    }
}

impl<T> BareOrValueSequenceConstraint<T> {
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
/// since the `BareOrValueSequenceConstraint<T>` type aims to be a
/// generalization over multiple types in the W3C spec:
///
/// | Rust                              | W3C                                                               |
/// | --------------------------------- | ----------------------------------------------------------------- |
/// | `ValueSequenceConstraint<String>` | [`ConstrainDOMStringParameters`][constrain_dom_string_parameters] |
///
/// [constrain_dom_string_parameters]: https://www.w3.org/TR/mediacapture-streams/#dom-constraindomstringparameters
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, Eq, PartialEq)]
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
    #[inline]
    pub fn exact<U>(mut self, exact: U) -> Self
    where
        Option<Vec<T>>: From<U>,
    {
        self.exact = exact.into();
        self
    }

    #[inline]
    pub fn ideal<U>(mut self, ideal: U) -> Self
    where
        Option<Vec<T>>: From<U>,
    {
        self.ideal = ideal.into();
        self
    }

    pub fn is_required(&self) -> bool {
        self.exact.is_some()
    }

    pub fn is_empty(&self) -> bool {
        let exact_is_empty = self.exact.as_ref().map_or(true, Vec::is_empty);
        let ideal_is_empty = self.ideal.as_ref().map_or(true, Vec::is_empty);
        exact_is_empty && ideal_is_empty
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
        let expected = ValueSequenceConstraint::default().exact(vec![true]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn resolve_to_basic() {
        let constraint = BareOrValueSequenceConstraint::Bare(vec![true]);
        let strategy = MediaTrackConstraintResolutionStrategy::BareToIdeal;
        let actual: ValueSequenceConstraint<bool> = constraint.into_resolved(strategy);
        let expected = ValueSequenceConstraint::default().ideal(vec![true]);

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
            fn exact_constraint() {
                let subject = Subject::Constraint(ValueSequenceConstraint::default().exact(vec![$($values.to_owned()),*]));
                let json = serde_json::json!({
                    "exact": [$($values),*],
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn ideal_constraint() {
                let subject = Subject::Constraint(ValueSequenceConstraint::default().ideal(vec![$($values.to_owned()),*]));
                let json = serde_json::json!({
                    "ideal": [$($values),*],
                });

                test_serde_symmetry!(subject: subject, json: json);
            }

            #[test]
            fn full_constraint() {
                let subject = Subject::Constraint(ValueSequenceConstraint::default().exact(vec![$($values.to_owned()),*]).ideal(vec![$($values.to_owned()),*]));
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
