use std::ops::Deref;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackSetting;

pub use self::{
    value::{ResolvedValueConstraint, ValueConstraint},
    value_range::{ResolvedValueRangeConstraint, ValueRangeConstraint},
    value_sequence::{ResolvedValueSequenceConstraint, ValueSequenceConstraint},
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
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct EmptyConstraint {}

impl std::fmt::Display for EmptyConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<empty>")
    }
}

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
pub enum MediaTrackConstraint {
    Empty(EmptyConstraint),
    // `IntegerRange` must be ordered before `FloatRange(…)` in order for
    // `serde` to decode the correct variant.
    IntegerRange(ValueRangeConstraint<u64>),
    FloatRange(ValueRangeConstraint<f64>),
    // `Bool` must be ordered after `IntegerRange(…)`/`FloatRange(…)` in order for
    // `serde` to decode the correct variant.
    Bool(ValueConstraint<bool>),
    // `StringSequence` must be ordered before `String(…)` in order for
    // `serde` to decode the correct variant.
    StringSequence(ValueSequenceConstraint<String>),
    String(ValueConstraint<String>),
}

impl Default for MediaTrackConstraint {
    fn default() -> Self {
        Self::Empty(EmptyConstraint {})
    }
}

// Bool constraint:

impl From<bool> for MediaTrackConstraint {
    fn from(bare: bool) -> Self {
        Self::Bool(bare.into())
    }
}

impl From<ResolvedValueConstraint<bool>> for MediaTrackConstraint {
    fn from(constraint: ResolvedValueConstraint<bool>) -> Self {
        Self::Bool(constraint.into())
    }
}

impl From<ValueConstraint<bool>> for MediaTrackConstraint {
    fn from(constraint: ValueConstraint<bool>) -> Self {
        Self::Bool(constraint)
    }
}

// Unsigned integer range constraint:

impl From<u64> for MediaTrackConstraint {
    fn from(bare: u64) -> Self {
        Self::IntegerRange(bare.into())
    }
}

impl From<ResolvedValueRangeConstraint<u64>> for MediaTrackConstraint {
    fn from(constraint: ResolvedValueRangeConstraint<u64>) -> Self {
        Self::IntegerRange(constraint.into())
    }
}

impl From<ValueRangeConstraint<u64>> for MediaTrackConstraint {
    fn from(constraint: ValueRangeConstraint<u64>) -> Self {
        Self::IntegerRange(constraint)
    }
}

// Floating-point range constraint:

impl From<f64> for MediaTrackConstraint {
    fn from(bare: f64) -> Self {
        Self::FloatRange(bare.into())
    }
}

impl From<ResolvedValueRangeConstraint<f64>> for MediaTrackConstraint {
    fn from(constraint: ResolvedValueRangeConstraint<f64>) -> Self {
        Self::FloatRange(constraint.into())
    }
}

impl From<ValueRangeConstraint<f64>> for MediaTrackConstraint {
    fn from(constraint: ValueRangeConstraint<f64>) -> Self {
        Self::FloatRange(constraint)
    }
}

// String sequence constraint:

impl From<Vec<String>> for MediaTrackConstraint {
    fn from(bare: Vec<String>) -> Self {
        Self::StringSequence(bare.into())
    }
}

impl From<Vec<&str>> for MediaTrackConstraint {
    fn from(bare: Vec<&str>) -> Self {
        let bare: Vec<String> = bare.into_iter().map(|c| c.to_owned()).collect();
        Self::from(bare)
    }
}

impl From<ResolvedValueSequenceConstraint<String>> for MediaTrackConstraint {
    fn from(constraint: ResolvedValueSequenceConstraint<String>) -> Self {
        Self::StringSequence(constraint.into())
    }
}

impl From<ValueSequenceConstraint<String>> for MediaTrackConstraint {
    fn from(constraint: ValueSequenceConstraint<String>) -> Self {
        Self::StringSequence(constraint)
    }
}

// String constraint:

impl From<String> for MediaTrackConstraint {
    fn from(bare: String) -> Self {
        Self::String(bare.into())
    }
}

impl<'a> From<&'a str> for MediaTrackConstraint {
    fn from(bare: &'a str) -> Self {
        let bare: String = bare.to_owned();
        Self::from(bare)
    }
}

impl From<ResolvedValueConstraint<String>> for MediaTrackConstraint {
    fn from(constraint: ResolvedValueConstraint<String>) -> Self {
        Self::String(constraint.into())
    }
}

impl From<ValueConstraint<String>> for MediaTrackConstraint {
    fn from(constraint: ValueConstraint<String>) -> Self {
        Self::String(constraint)
    }
}

// Conversion from settings:

impl From<MediaTrackSetting> for MediaTrackConstraint {
    fn from(settings: MediaTrackSetting) -> Self {
        match settings {
            MediaTrackSetting::Bool(value) => Self::Bool(value.into()),
            MediaTrackSetting::Integer(value) => {
                Self::IntegerRange((value.clamp(0, i64::MAX) as u64).into())
            }
            MediaTrackSetting::Float(value) => Self::FloatRange(value.into()),
            MediaTrackSetting::String(value) => Self::String(value.into()),
        }
    }
}

impl MediaTrackConstraint {
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
    ) -> ResolvedMediaTrackConstraint {
        self.clone().into_resolved(strategy)
    }

    pub fn into_resolved(
        self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> ResolvedMediaTrackConstraint {
        match self {
            Self::Empty(constraint) => ResolvedMediaTrackConstraint::Empty(constraint),
            Self::IntegerRange(constraint) => {
                ResolvedMediaTrackConstraint::IntegerRange(constraint.into_resolved(strategy))
            }
            Self::FloatRange(constraint) => {
                ResolvedMediaTrackConstraint::FloatRange(constraint.into_resolved(strategy))
            }
            Self::Bool(constraint) => {
                ResolvedMediaTrackConstraint::Bool(constraint.into_resolved(strategy))
            }
            Self::StringSequence(constraint) => {
                ResolvedMediaTrackConstraint::StringSequence(constraint.into_resolved(strategy))
            }
            Self::String(constraint) => {
                ResolvedMediaTrackConstraint::String(constraint.into_resolved(strategy))
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
pub enum ResolvedMediaTrackConstraint {
    Empty(EmptyConstraint),
    IntegerRange(ResolvedValueRangeConstraint<u64>),
    FloatRange(ResolvedValueRangeConstraint<f64>),
    Bool(ResolvedValueConstraint<bool>),
    StringSequence(ResolvedValueSequenceConstraint<String>),
    String(ResolvedValueConstraint<String>),
}

impl Default for ResolvedMediaTrackConstraint {
    fn default() -> Self {
        Self::Empty(EmptyConstraint {})
    }
}

impl std::fmt::Display for ResolvedMediaTrackConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty(constraint) => constraint.fmt(f),
            Self::IntegerRange(constraint) => constraint.fmt(f),
            Self::FloatRange(constraint) => constraint.fmt(f),
            Self::Bool(constraint) => constraint.fmt(f),
            Self::StringSequence(constraint) => constraint.fmt(f),
            Self::String(constraint) => constraint.fmt(f),
        }
    }
}

// Bool constraint:

impl From<ResolvedValueConstraint<bool>> for ResolvedMediaTrackConstraint {
    fn from(constraint: ResolvedValueConstraint<bool>) -> Self {
        Self::Bool(constraint)
    }
}

// Unsigned integer range constraint:

impl From<ResolvedValueRangeConstraint<u64>> for ResolvedMediaTrackConstraint {
    fn from(constraint: ResolvedValueRangeConstraint<u64>) -> Self {
        Self::IntegerRange(constraint)
    }
}

// Floating-point range constraint:

impl From<ResolvedValueRangeConstraint<f64>> for ResolvedMediaTrackConstraint {
    fn from(constraint: ResolvedValueRangeConstraint<f64>) -> Self {
        Self::FloatRange(constraint)
    }
}

// String sequence constraint:

impl From<ResolvedValueSequenceConstraint<String>> for ResolvedMediaTrackConstraint {
    fn from(constraint: ResolvedValueSequenceConstraint<String>) -> Self {
        Self::StringSequence(constraint)
    }
}

// String constraint:

impl From<ResolvedValueConstraint<String>> for ResolvedMediaTrackConstraint {
    fn from(constraint: ResolvedValueConstraint<String>) -> Self {
        Self::String(constraint)
    }
}

impl ResolvedMediaTrackConstraint {
    pub fn exact_from(setting: MediaTrackSetting) -> Self {
        MediaTrackConstraint::from(setting)
            .into_resolved(MediaTrackConstraintResolutionStrategy::BareToExact)
    }

    pub fn ideal_from(setting: MediaTrackSetting) -> Self {
        MediaTrackConstraint::from(setting)
            .into_resolved(MediaTrackConstraintResolutionStrategy::BareToIdeal)
    }

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

    pub fn to_required_only(&self) -> Self {
        self.clone().into_required_only()
    }

    pub fn into_required_only(self) -> Self {
        match self {
            Self::Empty(constraint) => Self::Empty(constraint),
            Self::IntegerRange(constraint) => Self::IntegerRange(constraint.into_required_only()),
            Self::FloatRange(constraint) => Self::FloatRange(constraint.into_required_only()),
            Self::Bool(constraint) => Self::Bool(constraint.into_required_only()),
            Self::StringSequence(constraint) => {
                Self::StringSequence(constraint.into_required_only())
            }
            Self::String(constraint) => Self::String(constraint.into_required_only()),
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
/// The wrapped `ResolvedMediaTrackConstraint` MUST not be empty.
///
/// To enforce this invariant the only way to create an instance of this type
/// is by calling `constraint.to_sanitized()`/`constraint.into_sanitized()` on
/// an instance of `ResolvedMediaTrackConstraint`, which returns `None` if `self` is empty.
///
/// Further more `self.0` MUST NOT be exposed mutably,
/// as otherwise it could become empty via mutation.
#[derive(Debug, Clone, PartialEq)]
pub struct SanitizedMediaTrackConstraint(ResolvedMediaTrackConstraint);

impl Deref for SanitizedMediaTrackConstraint {
    type Target = ResolvedMediaTrackConstraint;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl SanitizedMediaTrackConstraint {
    pub fn into_inner(self) -> ResolvedMediaTrackConstraint {
        self.0
    }

    pub fn integer_range(&self) -> Option<&ResolvedValueRangeConstraint<u64>> {
        if let ResolvedMediaTrackConstraint::IntegerRange(constraint) = &self.0 {
            Some(constraint)
        } else {
            None
        }
    }

    pub fn float_range(&self) -> Option<&ResolvedValueRangeConstraint<f64>> {
        if let ResolvedMediaTrackConstraint::FloatRange(constraint) = &self.0 {
            Some(constraint)
        } else {
            None
        }
    }

    pub fn bool(&self) -> Option<&ResolvedValueConstraint<bool>> {
        if let ResolvedMediaTrackConstraint::Bool(constraint) = &self.0 {
            Some(constraint)
        } else {
            None
        }
    }

    pub fn string_sequence(&self) -> Option<&ResolvedValueSequenceConstraint<String>> {
        if let ResolvedMediaTrackConstraint::StringSequence(constraint) = &self.0 {
            Some(constraint)
        } else {
            None
        }
    }

    pub fn string(&self) -> Option<&ResolvedValueConstraint<String>> {
        if let ResolvedMediaTrackConstraint::String(constraint) = &self.0 {
            Some(constraint)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    type Subject = MediaTrackConstraint;

    #[test]
    fn default() {
        let subject = Subject::default();

        let actual = subject.is_empty();
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[test]
    fn from_bool() {
        let subjects = [
            Subject::from(false),
            Subject::from(ValueConstraint::<bool>::default()),
            Subject::from(ResolvedValueConstraint::<bool>::default()),
        ];

        for subject in subjects {
            // TODO: replace with `assert_matches!(…)`, once stabilized:
            // Tracking issue: https://github.com/rust-lang/rust/issues/82775
            assert!(matches!(subject, Subject::Bool(_)));
        }
    }

    #[test]
    fn from_u64() {
        let subjects = [
            Subject::from(42_u64),
            Subject::from(ValueRangeConstraint::<u64>::default()),
            Subject::from(ResolvedValueRangeConstraint::<u64>::default()),
        ];

        for subject in subjects {
            // TODO: replace with `assert_matches!(…)`, once stabilized:
            // Tracking issue: https://github.com/rust-lang/rust/issues/82775
            assert!(matches!(subject, Subject::IntegerRange(_)));
        }
    }

    #[test]
    fn from_f64() {
        let subjects = [
            Subject::from(42.0_f64),
            Subject::from(ValueRangeConstraint::<f64>::default()),
            Subject::from(ResolvedValueRangeConstraint::<f64>::default()),
        ];

        for subject in subjects {
            // TODO: replace with `assert_matches!(…)`, once stabilized:
            // Tracking issue: https://github.com/rust-lang/rust/issues/82775
            assert!(matches!(subject, Subject::FloatRange(_)));
        }
    }

    #[test]
    fn from_string() {
        let subjects = [
            Subject::from(String::new()),
            Subject::from(ValueConstraint::<String>::default()),
            Subject::from(ResolvedValueConstraint::<String>::default()),
        ];

        for subject in subjects {
            // TODO: replace with `assert_matches!(…)`, once stabilized:
            // Tracking issue: https://github.com/rust-lang/rust/issues/82775
            assert!(matches!(subject, Subject::String(_)));
        }
    }

    #[test]
    fn from_string_sequence() {
        let subjects = [
            Subject::from(vec![String::new()]),
            Subject::from(ValueSequenceConstraint::<String>::default()),
            Subject::from(ResolvedValueSequenceConstraint::<String>::default()),
        ];

        for subject in subjects {
            // TODO: replace with `assert_matches!(…)`, once stabilized:
            // Tracking issue: https://github.com/rust-lang/rust/issues/82775
            assert!(matches!(subject, Subject::StringSequence(_)));
        }
    }

    #[test]
    fn is_empty() {
        let empty_subject = Subject::Empty(EmptyConstraint {});

        assert!(empty_subject.is_empty());

        let non_empty_subjects = [
            Subject::Bool(ValueConstraint::Bare(true)),
            Subject::FloatRange(ValueRangeConstraint::Bare(42.0)),
            Subject::IntegerRange(ValueRangeConstraint::Bare(42)),
            Subject::String(ValueConstraint::Bare(String::new())),
            Subject::StringSequence(ValueSequenceConstraint::Bare(vec![String::new()])),
        ];

        for non_empty_subject in non_empty_subjects {
            assert!(!non_empty_subject.is_empty());
        }
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::macros::test_serde_symmetry;

    use super::*;

    type Subject = MediaTrackConstraint;

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
        let subject = Subject::Bool(ResolvedValueConstraint::default().exact(true).into());
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
        let subject =
            Subject::IntegerRange(ResolvedValueRangeConstraint::default().exact(42).into());
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
        let subject =
            Subject::FloatRange(ResolvedValueRangeConstraint::default().exact(42.0).into());
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
            ResolvedValueSequenceConstraint::default()
                .exact(vec!["foo".to_owned(), "bar".to_owned()])
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
        let subject = Subject::String(
            ResolvedValueConstraint::default()
                .exact("foo".to_owned())
                .into(),
        );
        let json = serde_json::json!({ "exact": "foo" });

        test_serde_symmetry!(subject: subject, json: json);
    }
}
