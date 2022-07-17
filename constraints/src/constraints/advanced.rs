#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackConstraintSet;

/// The list of advanced constraint sets for a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaTrackConstraints.advanced`][media_track_constraints_advanced]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_constraints_advanced]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackconstraints-advanced
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct AdvancedMediaTrackConstraints(Vec<MediaTrackConstraintSet>);

impl AdvancedMediaTrackConstraints {
    pub fn new(constraints: Vec<MediaTrackConstraintSet>) -> Self {
        Self(constraints)
    }
}

impl FromIterator<MediaTrackConstraintSet> for AdvancedMediaTrackConstraints {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = MediaTrackConstraintSet>,
    {
        Self::new(iter.into_iter().collect())
    }
}

impl IntoIterator for AdvancedMediaTrackConstraints {
    type Item = MediaTrackConstraintSet;
    type IntoIter = std::vec::IntoIter<MediaTrackConstraintSet>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a AdvancedMediaTrackConstraints {
    type Item = &'a MediaTrackConstraintSet;
    type IntoIter = std::slice::Iter<'a, MediaTrackConstraintSet>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut AdvancedMediaTrackConstraints {
    type Item = &'a mut MediaTrackConstraintSet;
    type IntoIter = std::slice::IterMut<'a, MediaTrackConstraintSet>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl AdvancedMediaTrackConstraints {
    pub fn iter(&self) -> std::slice::Iter<'_, MediaTrackConstraintSet> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, MediaTrackConstraintSet> {
        self.0.iter_mut()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn push<T>(&mut self, constraint_set: MediaTrackConstraintSet) {
        self.0.push(constraint_set);
    }

    pub fn remove(&mut self, index: usize) -> MediaTrackConstraintSet {
        self.0.remove(index)
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::property::name::*;

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
        let advanced = AdvancedMediaTrackConstraints(vec![MediaTrackConstraintSet::from_iter([
            (DEVICE_ID, "device-id".into()),
            (AUTO_GAIN_CONTROL, true.into()),
            (CHANNEL_COUNT, 2.into()),
            (LATENCY, 0.123.into()),
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
        let expected = AdvancedMediaTrackConstraints(vec![MediaTrackConstraintSet::from_iter([
            (DEVICE_ID, "device-id".into()),
            (AUTO_GAIN_CONTROL, true.into()),
            (CHANNEL_COUNT, 2.into()),
            (LATENCY, 0.123.into()),
        ])]);

        assert_eq!(actual, expected);
    }
}
