use std::{
    iter::FromIterator,
    ops::{Deref, DerefMut},
};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    MediaTrackConstraint, MediaTrackConstraintResolutionStrategy, MediaTrackSupportedConstraints,
    ResolvedMediaTrackConstraint, SanitizedMediaTrackConstraint,
};

use super::constraint_set::GenericMediaTrackConstraintSet;

/// Advanced media track constraints that contain sets of either bare values or constraints.
pub type AdvancedMediaTrackConstraints = GenericAdvancedMediaTrackConstraints<MediaTrackConstraint>;

/// Advanced media track constraints that contain sets of constraints (both, empty and non-empty).
pub type ResolvedAdvancedMediaTrackConstraints =
    GenericAdvancedMediaTrackConstraints<ResolvedMediaTrackConstraint>;

/// Advanced media track constraints that contain sets of only non-empty constraints.
pub type SanitizedAdvancedMediaTrackConstraints =
    GenericAdvancedMediaTrackConstraints<SanitizedMediaTrackConstraint>;

/// The list of advanced constraint sets for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`ResolvedMediaTrackConstraints.advanced`][media_track_constraints_advanced]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraints_advanced]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraints-advanced
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct GenericAdvancedMediaTrackConstraints<T>(Vec<GenericMediaTrackConstraintSet<T>>);

impl<T> GenericAdvancedMediaTrackConstraints<T> {
    pub fn new(constraints: Vec<GenericMediaTrackConstraintSet<T>>) -> Self {
        Self(constraints)
    }

    pub fn into_inner(self) -> Vec<GenericMediaTrackConstraintSet<T>> {
        self.0
    }
}

impl<T> Deref for GenericAdvancedMediaTrackConstraints<T> {
    type Target = Vec<GenericMediaTrackConstraintSet<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for GenericAdvancedMediaTrackConstraints<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Default for GenericAdvancedMediaTrackConstraints<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T> FromIterator<GenericMediaTrackConstraintSet<T>>
    for GenericAdvancedMediaTrackConstraints<T>
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = GenericMediaTrackConstraintSet<T>>,
    {
        Self::new(iter.into_iter().collect())
    }
}

impl<T> IntoIterator for GenericAdvancedMediaTrackConstraints<T> {
    type Item = GenericMediaTrackConstraintSet<T>;
    type IntoIter = std::vec::IntoIter<GenericMediaTrackConstraintSet<T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl AdvancedMediaTrackConstraints {
    pub fn to_resolved(&self) -> ResolvedAdvancedMediaTrackConstraints {
        self.clone().into_resolved()
    }

    pub fn into_resolved(self) -> ResolvedAdvancedMediaTrackConstraints {
        let strategy = MediaTrackConstraintResolutionStrategy::BareToExact;
        ResolvedAdvancedMediaTrackConstraints::from_iter(
            self.into_iter()
                .map(|constraint_set| constraint_set.into_resolved(strategy)),
        )
    }
}

impl ResolvedAdvancedMediaTrackConstraints {
    pub fn to_sanitized(
        &self,
        supported_constraints: &MediaTrackSupportedConstraints,
    ) -> SanitizedAdvancedMediaTrackConstraints {
        self.clone().into_sanitized(supported_constraints)
    }

    pub fn into_sanitized(
        self,
        supported_constraints: &MediaTrackSupportedConstraints,
    ) -> SanitizedAdvancedMediaTrackConstraints {
        SanitizedAdvancedMediaTrackConstraints::from_iter(
            self.into_iter()
                .map(|constraint_set| constraint_set.into_sanitized(supported_constraints))
                .filter(|constraint_set| !constraint_set.is_empty()),
        )
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::{property::all::name::*, MediaTrackConstraintSet};

    use super::*;

    #[test]
    fn serialize_default() {
        let advanced = AdvancedMediaTrackConstraints::default();
        let actual = serde_json::to_value(advanced).unwrap();
        let expected = serde_json::json!([]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn deserialize_default() {
        let json = serde_json::json!([]);
        let actual: AdvancedMediaTrackConstraints = serde_json::from_value(json).unwrap();
        let expected = AdvancedMediaTrackConstraints::default();

        assert_eq!(actual, expected);
    }

    #[test]
    fn serialize() {
        let advanced =
            AdvancedMediaTrackConstraints::new(vec![MediaTrackConstraintSet::from_iter([
                (&DEVICE_ID, "device-id".into()),
                (&AUTO_GAIN_CONTROL, true.into()),
                (&CHANNEL_COUNT, 2.into()),
                (&LATENCY, 0.123.into()),
            ])]);
        let actual = serde_json::to_value(advanced).unwrap();
        let expected = serde_json::json!([
            {
                "deviceId": "device-id".to_owned(),
                "autoGainControl": true,
                "channelCount": 2,
                "latency": 0.123,
            }
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn deserialize() {
        let json = serde_json::json!([
            {
                "deviceId": "device-id".to_owned(),
                "autoGainControl": true,
                "channelCount": 2,
                "latency": 0.123,
            }
        ]);
        let actual: AdvancedMediaTrackConstraints = serde_json::from_value(json).unwrap();
        let expected =
            AdvancedMediaTrackConstraints::new(vec![MediaTrackConstraintSet::from_iter([
                (&DEVICE_ID, "device-id".into()),
                (&AUTO_GAIN_CONTROL, true.into()),
                (&CHANNEL_COUNT, 2.into()),
                (&LATENCY, 0.123.into()),
            ])]);

        assert_eq!(actual, expected);
    }
}
