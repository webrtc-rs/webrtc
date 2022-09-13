#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    constraint::SanitizedMediaTrackConstraint, BareOrMediaTrackConstraint, MediaTrackConstraint,
    MediaTrackConstraintResolutionStrategy, MediaTrackSupportedConstraints,
};

use super::{
    advanced::GenericAdvancedMediaTrackConstraints, constraint_set::GenericMediaTrackConstraintSet,
};

/// A boolean on/off flag or constraints for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `BoolOrMediaTrackConstraints<T>` type aims to be a
/// generalization over multiple types in the W3C spec:
///
/// | Rust                          | W3C                                                                                                |
/// | ----------------------------- | -------------------------------------------------------------------------------------------------- |
/// | `BoolOrMediaTrackConstraints` | [`MediaStreamConstraints`][media_stream_constraints]'s [`video`][video] / [`audio`][audio] members |
///
/// Unlike `BoolOrMediaTrackConstraints` this type does not contain constraints
/// with bare values, but has them resolved to full constraints instead.
///
/// [media_stream_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-video
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [video]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-video
/// [audio]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-audio
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
pub type BareOrBoolOrMediaTrackConstraints =
    GenericBoolOrMediaTrackConstraints<BareOrMediaTrackConstraint>;

/// A boolean on/off flag or constraints for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `BoolOrMediaTrackConstraints<T>` type aims to be a
/// generalization over multiple types in the W3C spec:
///
/// | Rust                          | W3C                                                                                                |
/// | ----------------------------- | -------------------------------------------------------------------------------------------------- |
/// | `BoolOrMediaTrackConstraints` | [`MediaStreamConstraints`][media_stream_constraints]'s [`video`][video] / [`audio`][audio] members |
///
/// Unlike `BareOrBoolOrMediaTrackConstraints` this type may contain constraints with bare values.
///
/// [media_stream_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-video
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [video]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-video
/// [audio]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-audio
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
pub type BoolOrMediaTrackConstraints = GenericBoolOrMediaTrackConstraints<MediaTrackConstraint>;

/// A boolean on/off flag or constraints for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// There exists no direct corresponding type in the
/// W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec,
/// since the `BoolOrMediaTrackConstraints<T>` type aims to be a
/// generalization over multiple types in the W3C spec:
///
/// | Rust                          | W3C                                                                                                |
/// | ----------------------------- | -------------------------------------------------------------------------------------------------- |
/// | `BoolOrMediaTrackConstraints` | [`MediaStreamConstraints`][media_stream_constraints]'s [`video`][video] / [`audio`][audio] members |
///
/// [media_stream_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-video
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [video]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-video
/// [audio]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-audio
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum GenericBoolOrMediaTrackConstraints<T> {
    Bool(bool),
    Constraints(GenericMediaTrackConstraints<T>),
}

impl<T> GenericBoolOrMediaTrackConstraints<T>
where
    T: Clone,
{
    pub fn to_constraints(&self) -> Option<GenericMediaTrackConstraints<T>> {
        self.clone().into_constraints()
    }

    pub fn into_constraints(self) -> Option<GenericMediaTrackConstraints<T>> {
        match self {
            Self::Bool(false) => None,
            Self::Bool(true) => Some(GenericMediaTrackConstraints::default()),
            Self::Constraints(constraints) => Some(constraints),
        }
    }
}

impl<T> Default for GenericBoolOrMediaTrackConstraints<T> {
    fn default() -> Self {
        Self::Bool(false)
    }
}

impl<T> From<bool> for GenericBoolOrMediaTrackConstraints<T> {
    fn from(flag: bool) -> Self {
        Self::Bool(flag)
    }
}

impl<T> From<GenericMediaTrackConstraints<T>> for GenericBoolOrMediaTrackConstraints<T> {
    fn from(constraints: GenericMediaTrackConstraints<T>) -> Self {
        Self::Constraints(constraints)
    }
}

impl BareOrBoolOrMediaTrackConstraints {
    pub fn to_resolved(&self) -> BoolOrMediaTrackConstraints {
        self.clone().into_resolved()
    }

    pub fn into_resolved(self) -> BoolOrMediaTrackConstraints {
        match self {
            Self::Bool(flag) => BoolOrMediaTrackConstraints::Bool(flag),
            Self::Constraints(constraints) => {
                BoolOrMediaTrackConstraints::Constraints(constraints.into_resolved())
            }
        }
    }
}

/// The constraints for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaTrackConstraints`][media_track_constraints]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraints
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
pub type BareOrMediaTrackConstraints = GenericMediaTrackConstraints<BareOrMediaTrackConstraint>;

/// The constraints for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaTrackConstraints`][media_track_constraints]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraints
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
pub type MediaTrackConstraints = GenericMediaTrackConstraints<MediaTrackConstraint>;

pub type SanitizedMediaTrackConstraints =
    GenericMediaTrackConstraints<SanitizedMediaTrackConstraint>;

/// The constraints for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaTrackConstraints`][media_track_constraints]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraints
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericMediaTrackConstraints<T> {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub basic_or_required: GenericMediaTrackConstraintSet<T>,

    #[cfg_attr(
        feature = "serde",
        serde(default = "Default::default"),
        serde(skip_serializing_if = "should_skip_advanced")
    )]
    pub advanced: GenericAdvancedMediaTrackConstraints<T>,
}

#[cfg(feature = "serde")]
fn should_skip_advanced<T>(advanced: &GenericAdvancedMediaTrackConstraints<T>) -> bool {
    advanced.is_empty()
}

impl<T> GenericMediaTrackConstraints<T> {
    pub fn new(
        basic_or_required: GenericMediaTrackConstraintSet<T>,
        advanced: GenericAdvancedMediaTrackConstraints<T>,
    ) -> Self {
        Self {
            basic_or_required,
            advanced,
        }
    }
}

impl GenericMediaTrackConstraints<MediaTrackConstraint> {
    pub fn basic(&self) -> GenericMediaTrackConstraintSet<MediaTrackConstraint> {
        self.basic_or_required(false)
    }

    pub fn required(&self) -> GenericMediaTrackConstraintSet<MediaTrackConstraint> {
        self.basic_or_required(true)
    }

    fn basic_or_required(
        &self,
        required: bool,
    ) -> GenericMediaTrackConstraintSet<MediaTrackConstraint> {
        GenericMediaTrackConstraintSet::new(
            self.basic_or_required
                .iter()
                .filter_map(|(property, constraint)| {
                    if constraint.is_required() == required {
                        Some((property.clone(), constraint.clone()))
                    } else {
                        None
                    }
                })
                .collect(),
        )
    }
}

impl<T> Default for GenericMediaTrackConstraints<T> {
    fn default() -> Self {
        Self {
            basic_or_required: Default::default(),
            advanced: Default::default(),
        }
    }
}

impl From<BareOrMediaTrackConstraints> for MediaTrackConstraints {
    fn from(bare_or_constraints: BareOrMediaTrackConstraints) -> Self {
        bare_or_constraints.into_resolved()
    }
}

impl BareOrMediaTrackConstraints {
    pub fn to_resolved(&self) -> MediaTrackConstraints {
        self.clone().into_resolved()
    }

    pub fn into_resolved(self) -> MediaTrackConstraints {
        let Self {
            basic_or_required,
            advanced,
        } = self;
        MediaTrackConstraints {
            basic_or_required: basic_or_required
                .into_resolved(MediaTrackConstraintResolutionStrategy::BareToIdeal),
            advanced: advanced.into_resolved(),
        }
    }
}

impl MediaTrackConstraints {
    pub fn to_sanitized(
        &self,
        supported_constraints: &MediaTrackSupportedConstraints,
    ) -> SanitizedMediaTrackConstraints {
        self.clone().into_sanitized(supported_constraints)
    }

    pub fn into_sanitized(
        self,
        supported_constraints: &MediaTrackSupportedConstraints,
    ) -> SanitizedMediaTrackConstraints {
        let basic_or_required = self.basic_or_required.into_sanitized(supported_constraints);
        let advanced: GenericAdvancedMediaTrackConstraints<_> = self
            .advanced
            .into_iter()
            .map(|constraint_set| constraint_set.into_sanitized(supported_constraints))
            .filter(|constraint_set| !constraint_set.is_empty())
            .collect();

        SanitizedMediaTrackConstraints {
            basic_or_required,
            advanced,
        }
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::{
        macros::test_serde_symmetry, property::name::*, BareOrAdvancedMediaTrackConstraints,
        BareOrMediaTrackConstraintSet,
    };

    use super::*;

    type Subject = BareOrMediaTrackConstraints;

    #[test]
    fn default() {
        let subject = Subject::default();
        let json = serde_json::json!({});

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn customized() {
        let subject = Subject {
            basic_or_required: BareOrMediaTrackConstraintSet::from_iter([(
                DEVICE_ID,
                "microphone".into(),
            )]),
            advanced: BareOrAdvancedMediaTrackConstraints::new(vec![
                BareOrMediaTrackConstraintSet::from_iter([
                    (AUTO_GAIN_CONTROL, true.into()),
                    (CHANNEL_COUNT, 2.into()),
                ]),
                BareOrMediaTrackConstraintSet::from_iter([(LATENCY, 0.123.into())]),
            ]),
        };
        let json = serde_json::json!({
            "deviceId": "microphone",
            "advanced": [
                {
                    "autoGainControl": true,
                    "channelCount": 2,
                },
                {
                    "latency": 0.123,
                },
            ]
        });

        test_serde_symmetry!(subject: subject, json: json);
    }
}
