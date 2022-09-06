use std::ops::Deref;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub use self::{
    value::{BareOrValueConstraint, ValueConstraint},
    value_range::{BareOrValueRangeConstraint, ValueRangeConstraint},
    value_sequence::{BareOrValueSequenceConstraint, ValueSequenceConstraint},
};

mod value;
mod value_range;
mod value_sequence;

/// An empty [constraint][media_track_constraints] value for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// There exists no corresponding type in the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// The purpose of this type is to reduce parsing ambiguity, since all constraint variant types
/// support serializing from an empty map, but an empty map isn't typed, really,
/// so parsing to a specifically typed constraint would be wrong, type-wise.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraints
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct EmptyConstraint {}

/// The strategy of a track [constraint][constraint].
///
/// [constraint]: https://www.w3.org/TR/mediacapture-streams/#dfn-constraint
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum MediaTrackConstraintResolutionStrategy {
    /// Resolve bare values to `ideal` constraints.
    BareToIdeal,
    /// Resolve bare values to `exact` constraints.
    BareToExact,
}

/// A single [constraint][media_track_constraints] value for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// There exists no corresponding type in the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraints
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum BareOrMediaTrackConstraint {
    Empty(EmptyConstraint),
    // `IntegerRange` must be ordered before `FloatRange(…)` in order for
    // `serde` to decode the correct variant.
    IntegerRange(BareOrValueRangeConstraint<u64>),
    FloatRange(BareOrValueRangeConstraint<f64>),
    // `Bool` must be ordered after `IntegerRange(…)`/`FloatRange(…)` in order for
    // `serde` to decode the correct variant.
    Bool(BareOrValueConstraint<bool>),
    // `StringSequence` must be ordered before `String(…)` in order for
    // `serde` to decode the correct variant.
    StringSequence(BareOrValueSequenceConstraint<String>),
    String(BareOrValueConstraint<String>),
}

impl Default for BareOrMediaTrackConstraint {
    fn default() -> Self {
        Self::Empty(EmptyConstraint {})
    }
}

// Bool constraint:

impl From<bool> for BareOrMediaTrackConstraint {
    fn from(bare: bool) -> Self {
        Self::Bool(bare.into())
    }
}

impl From<ValueConstraint<bool>> for BareOrMediaTrackConstraint {
    fn from(constraint: ValueConstraint<bool>) -> Self {
        Self::Bool(constraint.into())
    }
}

impl From<BareOrValueConstraint<bool>> for BareOrMediaTrackConstraint {
    fn from(constraint: BareOrValueConstraint<bool>) -> Self {
        Self::Bool(constraint)
    }
}

// Unsigned integer range constraint:

impl From<u64> for BareOrMediaTrackConstraint {
    fn from(bare: u64) -> Self {
        Self::IntegerRange(bare.into())
    }
}

impl From<ValueRangeConstraint<u64>> for BareOrMediaTrackConstraint {
    fn from(constraint: ValueRangeConstraint<u64>) -> Self {
        Self::IntegerRange(constraint.into())
    }
}

impl From<BareOrValueRangeConstraint<u64>> for BareOrMediaTrackConstraint {
    fn from(constraint: BareOrValueRangeConstraint<u64>) -> Self {
        Self::IntegerRange(constraint)
    }
}

// Floating-point range constraint:

impl From<f64> for BareOrMediaTrackConstraint {
    fn from(bare: f64) -> Self {
        Self::FloatRange(bare.into())
    }
}

impl From<ValueRangeConstraint<f64>> for BareOrMediaTrackConstraint {
    fn from(constraint: ValueRangeConstraint<f64>) -> Self {
        Self::FloatRange(constraint.into())
    }
}

impl From<BareOrValueRangeConstraint<f64>> for BareOrMediaTrackConstraint {
    fn from(constraint: BareOrValueRangeConstraint<f64>) -> Self {
        Self::FloatRange(constraint)
    }
}

// String sequence constraint:

impl From<Vec<String>> for BareOrMediaTrackConstraint {
    fn from(bare: Vec<String>) -> Self {
        Self::StringSequence(bare.into())
    }
}

impl From<Vec<&str>> for BareOrMediaTrackConstraint {
    fn from(bare: Vec<&str>) -> Self {
        let bare: Vec<String> = bare.into_iter().map(|c| c.to_owned()).collect();
        Self::from(bare)
    }
}

impl From<ValueSequenceConstraint<String>> for BareOrMediaTrackConstraint {
    fn from(constraint: ValueSequenceConstraint<String>) -> Self {
        Self::StringSequence(constraint.into())
    }
}

impl From<BareOrValueSequenceConstraint<String>> for BareOrMediaTrackConstraint {
    fn from(constraint: BareOrValueSequenceConstraint<String>) -> Self {
        Self::StringSequence(constraint)
    }
}

// String constraint:

impl From<String> for BareOrMediaTrackConstraint {
    fn from(bare: String) -> Self {
        Self::String(bare.into())
    }
}

impl<'a> From<&'a str> for BareOrMediaTrackConstraint {
    fn from(bare: &'a str) -> Self {
        let bare: String = bare.to_owned();
        Self::from(bare)
    }
}

impl From<ValueConstraint<String>> for BareOrMediaTrackConstraint {
    fn from(constraint: ValueConstraint<String>) -> Self {
        Self::String(constraint.into())
    }
}

impl From<BareOrValueConstraint<String>> for BareOrMediaTrackConstraint {
    fn from(constraint: BareOrValueConstraint<String>) -> Self {
        Self::String(constraint)
    }
}

impl BareOrMediaTrackConstraint {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Empty(_) => true,
            Self::IntegerRange(constraint) => constraint.is_empty(),
            Self::FloatRange(constraint) => constraint.is_empty(),
            Self::Bool(constraint) => constraint.is_empty(),
            Self::StringSequence(constraint) => constraint.is_empty(),
            Self::String(constraint) => constraint.is_empty(),
        }
    }

    pub fn to_resolved(
        &self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> MediaTrackConstraint {
        self.clone().into_resolved(strategy)
    }

    pub fn into_resolved(
        self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> MediaTrackConstraint {
        match self {
            Self::Empty(constraint) => MediaTrackConstraint::Empty(constraint),
            Self::IntegerRange(constraint) => {
                MediaTrackConstraint::IntegerRange(constraint.into_resolved(strategy))
            }
            Self::FloatRange(constraint) => {
                MediaTrackConstraint::FloatRange(constraint.into_resolved(strategy))
            }
            Self::Bool(constraint) => {
                MediaTrackConstraint::Bool(constraint.into_resolved(strategy))
            }
            Self::StringSequence(constraint) => {
                MediaTrackConstraint::StringSequence(constraint.into_resolved(strategy))
            }
            Self::String(constraint) => {
                MediaTrackConstraint::String(constraint.into_resolved(strategy))
            }
        }
    }
}

/// A single [constraint][media_track_constraints] value for a [`MediaStreamTrack`][media_stream_track] object
/// with its potential bare value either resolved to an `exact` or `ideal` constraint.
///
/// # W3C Spec Compliance
///
/// There exists no corresponding type in the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraints
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum MediaTrackConstraint {
    Empty(EmptyConstraint),
    IntegerRange(ValueRangeConstraint<u64>),
    FloatRange(ValueRangeConstraint<f64>),
    Bool(ValueConstraint<bool>),
    StringSequence(ValueSequenceConstraint<String>),
    String(ValueConstraint<String>),
}

impl Default for MediaTrackConstraint {
    fn default() -> Self {
        Self::Empty(EmptyConstraint {})
    }
}

impl MediaTrackConstraint {
    pub fn is_required(&self) -> bool {
        match self {
            Self::Empty(_constraint) => false,
            Self::IntegerRange(constraint) => constraint.is_required(),
            Self::FloatRange(constraint) => constraint.is_required(),
            Self::Bool(constraint) => constraint.is_required(),
            Self::StringSequence(constraint) => constraint.is_required(),
            Self::String(constraint) => constraint.is_required(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Empty(_constraint) => true,
            Self::IntegerRange(constraint) => constraint.is_empty(),
            Self::FloatRange(constraint) => constraint.is_empty(),
            Self::Bool(constraint) => constraint.is_empty(),
            Self::StringSequence(constraint) => constraint.is_empty(),
            Self::String(constraint) => constraint.is_empty(),
        }
    }

    pub fn to_sanitized(&self) -> Option<SanitizedMediaTrackConstraint> {
        self.clone().into_sanitized()
    }

    pub fn into_sanitized(self) -> Option<SanitizedMediaTrackConstraint> {
        if self.is_empty() {
            return None;
        }

        Some(SanitizedMediaTrackConstraint(self))
    }
}

/// A single non-empty [constraint][media_track_constraints] value for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # Invariant
///
/// The wrapped `MediaTrackConstraint` MUST not be empty.
///
/// To enforce this invariant the only way to create an instance of this type
/// is by calling `constraint.to_sanitized()`/`constraint.into_sanitized()` on
/// an instance of `MediaTrackConstraint`, which returns `None` if `self` is empty.
///
/// Further more `self.0` MUST NOT be exposed mutably,
/// as otherwise it could become empty via mutation.
#[derive(Debug, Clone, PartialEq)]
pub struct SanitizedMediaTrackConstraint(MediaTrackConstraint);

impl Deref for SanitizedMediaTrackConstraint {
    type Target = MediaTrackConstraint;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl SanitizedMediaTrackConstraint {
    pub fn into_inner(self) -> MediaTrackConstraint {
        self.0
    }

    pub fn integer_range(&self) -> Option<&ValueRangeConstraint<u64>> {
        if let MediaTrackConstraint::IntegerRange(constraint) = &self.0 {
            Some(constraint)
        } else {
            None
        }
    }

    pub fn float_range(&self) -> Option<&ValueRangeConstraint<f64>> {
        if let MediaTrackConstraint::FloatRange(constraint) = &self.0 {
            Some(constraint)
        } else {
            None
        }
    }

    pub fn bool(&self) -> Option<&ValueConstraint<bool>> {
        if let MediaTrackConstraint::Bool(constraint) = &self.0 {
            Some(constraint)
        } else {
            None
        }
    }

    pub fn string_sequence(&self) -> Option<&ValueSequenceConstraint<String>> {
        if let MediaTrackConstraint::StringSequence(constraint) = &self.0 {
            Some(constraint)
        } else {
            None
        }
    }

    pub fn string(&self) -> Option<&ValueConstraint<String>> {
        if let MediaTrackConstraint::String(constraint) = &self.0 {
            Some(constraint)
        } else {
            None
        }
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    type Subject = BareOrMediaTrackConstraint;

    #[test]
    fn empty() {
        let subject = Subject::Empty(EmptyConstraint {});
        let json = serde_json::json!({});

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn bool_bare() {
        let subject = Subject::Bool(true.into());
        let json = serde_json::json!(true);

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn bool_constraint() {
        let subject = Subject::Bool(ValueConstraint::exact_only(true).into());
        let json = serde_json::json!({ "exact": true });

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn integer_range_bare() {
        let subject = Subject::IntegerRange(42.into());
        let json = serde_json::json!(42);

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn integer_range_constraint() {
        let subject = Subject::IntegerRange(ValueRangeConstraint::exact_only(42).into());
        let json = serde_json::json!({ "exact": 42 });

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn float_range_bare() {
        let subject = Subject::FloatRange(4.2.into());
        let json = serde_json::json!(4.2);

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn float_range_constraint() {
        let subject = Subject::FloatRange(ValueRangeConstraint::exact_only(42.0).into());
        let json = serde_json::json!({ "exact": 42.0 });

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn string_sequence_bare() {
        let subject = Subject::StringSequence(vec!["foo".to_owned(), "bar".to_owned()].into());
        let json = serde_json::json!(["foo", "bar"]);

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn string_sequence_constraint() {
        let subject = Subject::StringSequence(
            ValueSequenceConstraint::exact_only(vec!["foo".to_owned(), "bar".to_owned()].into())
                .into(),
        );
        let json = serde_json::json!({ "exact": ["foo", "bar"] });

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn string_bare() {
        let subject = Subject::String("foo".to_owned().into());
        let json = serde_json::json!("foo");

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn string_constraint() {
        let subject = Subject::String(ValueConstraint::exact_only("foo".to_owned()).into());
        let json = serde_json::json!({ "exact": "foo" });

        test_serde_symmetry!(subject: subject, json: json);
    }
}
