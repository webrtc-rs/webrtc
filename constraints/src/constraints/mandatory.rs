use std::{
    iter::FromIterator,
    ops::{Deref, DerefMut},
};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    MediaTrackConstraint, MediaTrackConstraintResolutionStrategy, MediaTrackProperty,
    MediaTrackSupportedConstraints, ResolvedMediaTrackConstraint, SanitizedMediaTrackConstraint,
};

use super::constraint_set::GenericMediaTrackConstraintSet;

/// The list of mandatory constraint sets for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`ResolvedMediaTrackConstraints.mandatory`][media_track_constraints_mandatory]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// Unlike `ResolvedMandatoryMediaTrackConstraints` this type may contain constraints with bare values.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraints_mandatory]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraints-mandatory
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
pub type MandatoryMediaTrackConstraints =
    GenericMandatoryMediaTrackConstraints<MediaTrackConstraint>;

/// The list of mandatory constraint sets for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`ResolvedMediaTrackConstraintSet`][media_track_constraints_mandatory]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// Unlike `MandatoryMediaTrackConstraints` this type does not contain constraints
/// with bare values, but has them resolved to full constraints instead.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraints_mandatory]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraints-mandatory
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
pub type ResolvedMandatoryMediaTrackConstraints =
    GenericMandatoryMediaTrackConstraints<ResolvedMediaTrackConstraint>;

/// Set of mandatory media track constraints that contains only non-empty constraints.
pub type SanitizedMandatoryMediaTrackConstraints =
    GenericMandatoryMediaTrackConstraints<SanitizedMediaTrackConstraint>;

/// The set of constraints for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`ResolvedMediaTrackConstraintSet`][media_track_constraint_set]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraint_set]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraintset
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct GenericMandatoryMediaTrackConstraints<T>(GenericMediaTrackConstraintSet<T>);

impl<T> GenericMandatoryMediaTrackConstraints<T> {
    pub fn new(constraints: GenericMediaTrackConstraintSet<T>) -> Self {
        Self(constraints)
    }

    pub fn into_inner(self) -> GenericMediaTrackConstraintSet<T> {
        self.0
    }
}

impl GenericMandatoryMediaTrackConstraints<ResolvedMediaTrackConstraint> {
    pub fn basic(&self) -> GenericMediaTrackConstraintSet<ResolvedMediaTrackConstraint> {
        self.basic_or_required(false)
    }

    pub fn required(&self) -> GenericMediaTrackConstraintSet<ResolvedMediaTrackConstraint> {
        self.basic_or_required(true)
    }

    fn basic_or_required(
        &self,
        required: bool,
    ) -> GenericMediaTrackConstraintSet<ResolvedMediaTrackConstraint> {
        GenericMediaTrackConstraintSet::new(
            self.0
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

impl<T> Deref for GenericMandatoryMediaTrackConstraints<T> {
    type Target = GenericMediaTrackConstraintSet<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for GenericMandatoryMediaTrackConstraints<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Default for GenericMandatoryMediaTrackConstraints<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T, U> FromIterator<(U, T)> for GenericMandatoryMediaTrackConstraints<T>
where
    U: Into<MediaTrackProperty>,
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (U, T)>,
    {
        Self::new(iter.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

impl<T> IntoIterator for GenericMandatoryMediaTrackConstraints<T> {
    type Item = (MediaTrackProperty, T);
    type IntoIter = indexmap::map::IntoIter<MediaTrackProperty, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl MandatoryMediaTrackConstraints {
    pub fn to_resolved(&self) -> ResolvedMandatoryMediaTrackConstraints {
        self.clone().into_resolved()
    }

    pub fn into_resolved(self) -> ResolvedMandatoryMediaTrackConstraints {
        let strategy = MediaTrackConstraintResolutionStrategy::BareToIdeal;
        ResolvedMandatoryMediaTrackConstraints::new(self.0.into_resolved(strategy))
    }
}

impl ResolvedMandatoryMediaTrackConstraints {
    pub fn to_sanitized(
        &self,
        supported_constraints: &MediaTrackSupportedConstraints,
    ) -> SanitizedMandatoryMediaTrackConstraints {
        self.clone().into_sanitized(supported_constraints)
    }

    pub fn into_sanitized(
        self,
        supported_constraints: &MediaTrackSupportedConstraints,
    ) -> SanitizedMandatoryMediaTrackConstraints {
        SanitizedMandatoryMediaTrackConstraints::new(self.0.into_sanitized(supported_constraints))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        property::all::name::*, ResolvedMediaTrackConstraintSet, ResolvedValueConstraint,
        ResolvedValueRangeConstraint,
    };

    use super::*;

    #[test]
    fn basic() {
        let mandatory = ResolvedMandatoryMediaTrackConstraints::new(
            ResolvedMediaTrackConstraintSet::from_iter([
                (
                    &DEVICE_ID,
                    ResolvedValueConstraint::default()
                        .exact("device-id".to_owned())
                        .into(),
                ),
                (
                    &AUTO_GAIN_CONTROL,
                    ResolvedValueConstraint::default().ideal(true).into(),
                ),
                (
                    &CHANNEL_COUNT,
                    ResolvedValueRangeConstraint::default()
                        .exact(2)
                        .ideal(3)
                        .into(),
                ),
            ]),
        );

        let actual = mandatory.basic();
        let expected = ResolvedMediaTrackConstraintSet::from_iter([(
            &AUTO_GAIN_CONTROL,
            ResolvedValueConstraint::default().ideal(true).into(),
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn required() {
        let mandatory = ResolvedMandatoryMediaTrackConstraints::new(
            ResolvedMediaTrackConstraintSet::from_iter([
                (
                    &DEVICE_ID,
                    ResolvedValueConstraint::default()
                        .exact("device-id".to_owned())
                        .into(),
                ),
                (
                    &AUTO_GAIN_CONTROL,
                    ResolvedValueConstraint::default().ideal(true).into(),
                ),
                (
                    &CHANNEL_COUNT,
                    ResolvedValueRangeConstraint::default()
                        .exact(2)
                        .ideal(3)
                        .into(),
                ),
            ]),
        );

        let actual = mandatory.required();
        let expected = ResolvedMediaTrackConstraintSet::from_iter([
            (
                &DEVICE_ID,
                ResolvedValueConstraint::default()
                    .exact("device-id".to_owned())
                    .into(),
            ),
            (
                &CHANNEL_COUNT,
                ResolvedValueRangeConstraint::default()
                    .exact(2)
                    .ideal(3)
                    .into(),
            ),
        ]);

        assert_eq!(actual, expected);
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::{property::all::name::*, MediaTrackConstraintSet};

    use super::*;

    #[test]
    fn serialize_default() {
        let mandatory = MandatoryMediaTrackConstraints::default();
        let actual = serde_json::to_value(mandatory).unwrap();
        let expected = serde_json::json!({});

        assert_eq!(actual, expected);
    }

    #[test]
    fn deserialize_default() {
        let json = serde_json::json!({});
        let actual: MandatoryMediaTrackConstraints = serde_json::from_value(json).unwrap();
        let expected = MandatoryMediaTrackConstraints::default();

        assert_eq!(actual, expected);
    }

    #[test]
    fn serialize() {
        let mandatory = MandatoryMediaTrackConstraints::new(MediaTrackConstraintSet::from_iter([
            (&DEVICE_ID, "device-id".into()),
            (&AUTO_GAIN_CONTROL, true.into()),
            (&CHANNEL_COUNT, 2.into()),
            (&LATENCY, 0.123.into()),
        ]));
        let actual = serde_json::to_value(mandatory).unwrap();
        let expected = serde_json::json!(
            {
                "deviceId": "device-id".to_owned(),
                "autoGainControl": true,
                "channelCount": 2,
                "latency": 0.123,
            }
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn deserialize() {
        let json = serde_json::json!(
            {
                "deviceId": "device-id".to_owned(),
                "autoGainControl": true,
                "channelCount": 2,
                "latency": 0.123,
            }
        );
        let actual: MandatoryMediaTrackConstraints = serde_json::from_value(json).unwrap();
        let expected = MandatoryMediaTrackConstraints::new(MediaTrackConstraintSet::from_iter([
            (&DEVICE_ID, "device-id".into()),
            (&AUTO_GAIN_CONTROL, true.into()),
            (&CHANNEL_COUNT, 2.into()),
            (&LATENCY, 0.123.into()),
        ]));

        assert_eq!(actual, expected);
    }
}
