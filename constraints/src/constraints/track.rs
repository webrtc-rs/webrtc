#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{BareOrMediaTrackConstraint, MediaTrackConstraint};

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
/// [media_stream_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-video
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [video]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-video
/// [audio]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints-audio
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum BoolOrMediaTrackConstraints {
    Bool(bool),
    Constraints(BareOrMediaTrackConstraints),
}

impl BoolOrMediaTrackConstraints {
    pub fn to_constraints(&self) -> Option<BareOrMediaTrackConstraints> {
        self.clone().into_constraints()
    }

    pub fn into_constraints(self) -> Option<BareOrMediaTrackConstraints> {
        match self {
            Self::Bool(false) => None,
            Self::Bool(true) => Some(BareOrMediaTrackConstraints::default()),
            Self::Constraints(constraints) => Some(constraints),
        }
    }
}

impl Default for BoolOrMediaTrackConstraints {
    fn default() -> Self {
        Self::Bool(false)
    }
}

impl From<bool> for BoolOrMediaTrackConstraints {
    fn from(flag: bool) -> Self {
        Self::Bool(flag)
    }
}

impl From<BareOrMediaTrackConstraints> for BoolOrMediaTrackConstraints {
    fn from(constraints: BareOrMediaTrackConstraints) -> Self {
        Self::Constraints(constraints)
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
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenericMediaTrackConstraints<T> {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub basic: GenericMediaTrackConstraintSet<T>,

    #[cfg_attr(
        feature = "serde",
        serde(default),
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
        basic: GenericMediaTrackConstraintSet<T>,
        advanced: GenericAdvancedMediaTrackConstraints<T>,
    ) -> Self {
        Self { basic, advanced }
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
            basic: BareOrMediaTrackConstraintSet::from_iter([(DEVICE_ID, "microphone".into())]),
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
