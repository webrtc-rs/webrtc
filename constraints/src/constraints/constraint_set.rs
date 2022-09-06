use std::ops::{Deref, DerefMut};

use indexmap::IndexMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    constraint::SanitizedMediaTrackConstraint, BareOrMediaTrackConstraint, MediaTrackConstraint,
    MediaTrackConstraintResolutionStrategy, MediaTrackSupportedConstraints,
};

/// Media track constraint set that contains either bare values or constraints.
pub type BareOrMediaTrackConstraintSet = GenericMediaTrackConstraintSet<BareOrMediaTrackConstraint>;

/// Media track constraint set that contains only constraints (both, empty and non-empty).
pub type MediaTrackConstraintSet = GenericMediaTrackConstraintSet<MediaTrackConstraint>;

/// Media track constraint set that contains only non-empty constraints.
pub type SanitizedMediaTrackConstraintSet =
    GenericMediaTrackConstraintSet<SanitizedMediaTrackConstraint>;

/// The set of constraints for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaTrackConstraintSet`][media_track_constraint_set]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraint_set]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraintset
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct GenericMediaTrackConstraintSet<T>(IndexMap<String, T>);

impl<T> GenericMediaTrackConstraintSet<T> {
    pub fn new(constraint_set: IndexMap<String, T>) -> Self {
        Self(constraint_set)
    }

    pub fn into_inner(self) -> IndexMap<String, T> {
        self.0
    }
}

impl<T> Deref for GenericMediaTrackConstraintSet<T> {
    type Target = IndexMap<String, T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for GenericMediaTrackConstraintSet<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Default for GenericMediaTrackConstraintSet<T> {
    fn default() -> Self {
        Self(IndexMap::new())
    }
}

impl<T, U> FromIterator<(U, T)> for GenericMediaTrackConstraintSet<T>
where
    U: Into<String>,
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (U, T)>,
    {
        Self::new(iter.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

impl<T> IntoIterator for GenericMediaTrackConstraintSet<T> {
    type Item = (String, T);
    type IntoIter = indexmap::map::IntoIter<String, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl BareOrMediaTrackConstraintSet {
    pub fn to_resolved(
        &self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> MediaTrackConstraintSet {
        self.clone().into_resolved(strategy)
    }

    pub fn into_resolved(
        self,
        strategy: MediaTrackConstraintResolutionStrategy,
    ) -> MediaTrackConstraintSet {
        MediaTrackConstraintSet::new(
            self.into_iter()
                .map(|(property, constraint)| (property, constraint.into_resolved(strategy)))
                .collect(),
        )
    }
}

impl MediaTrackConstraintSet {
    pub fn to_sanitized(
        &self,
        supported_constraints: &MediaTrackSupportedConstraints,
    ) -> SanitizedMediaTrackConstraintSet {
        self.clone().into_sanitized(supported_constraints)
    }

    pub fn into_sanitized(
        self,
        supported_constraints: &MediaTrackSupportedConstraints,
    ) -> SanitizedMediaTrackConstraintSet {
        let index_map: IndexMap<String, _> = self
            .into_iter()
            .filter_map(|(property, constraint)| {
                if supported_constraints.contains(&property) {
                    constraint
                        .into_sanitized()
                        .map(|constraint| (property, constraint))
                } else {
                    None
                }
            })
            .collect();
        SanitizedMediaTrackConstraintSet::new(index_map)
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::property::name::*;

    use super::*;

    #[test]
    fn serialize_default() {
        let constraint_set = BareOrMediaTrackConstraintSet::default();
        let actual = serde_json::to_value(constraint_set).unwrap();
        let expected = serde_json::json!({});

        assert_eq!(actual, expected);
    }

    #[test]
    fn deserialize_default() {
        let json = serde_json::json!({});
        let actual: BareOrMediaTrackConstraintSet = serde_json::from_value(json).unwrap();
        let expected = BareOrMediaTrackConstraintSet::default();

        assert_eq!(actual, expected);
    }

    #[test]
    fn serialize() {
        let constraint_set = BareOrMediaTrackConstraintSet::from_iter([
            (DEVICE_ID, "device-id".into()),
            (AUTO_GAIN_CONTROL, true.into()),
            (CHANNEL_COUNT, 2.into()),
            (LATENCY, 0.123.into()),
        ]);
        let actual = serde_json::to_value(constraint_set).unwrap();
        let expected = serde_json::json!({
            "deviceId": "device-id".to_owned(),
            "autoGainControl": true,
            "channelCount": 2,
            "latency": 0.123,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn deserialize() {
        let json = serde_json::json!({
            "deviceId": "device-id".to_owned(),
            "autoGainControl": true,
            "channelCount": 2,
            "latency": 0.123,
        });
        let actual: BareOrMediaTrackConstraintSet = serde_json::from_value(json).unwrap();
        let expected = BareOrMediaTrackConstraintSet::from_iter([
            (DEVICE_ID, "device-id".into()),
            (AUTO_GAIN_CONTROL, true.into()),
            (CHANNEL_COUNT, 2.into()),
            (LATENCY, 0.123.into()),
        ]);

        assert_eq!(actual, expected);
    }
}
