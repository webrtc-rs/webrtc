mod value;
mod value_range;
mod value_sequence;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub use self::{
    value::{BareOrValueConstraint, ValueConstraint},
    value_range::{BareOrValueRangeConstraint, ValueRangeConstraint},
    value_sequence::{BareOrValueSequenceConstraint, ValueSequenceConstraint},
};

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
