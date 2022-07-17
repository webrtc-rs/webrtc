use indexmap::IndexMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{BareOrMediaTrackConstraint, MediaTrackConstraint};

pub type BareOrMediaTrackConstraintSet = GenericMediaTrackConstraintSet<BareOrMediaTrackConstraint>;
pub type MediaTrackConstraintSet = GenericMediaTrackConstraintSet<MediaTrackConstraint>;

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

impl<'a, T> IntoIterator for &'a GenericMediaTrackConstraintSet<T> {
    type Item = (&'a String, &'a T);
    type IntoIter = indexmap::map::Iter<'a, String, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut GenericMediaTrackConstraintSet<T> {
    type Item = (&'a String, &'a mut T);
    type IntoIter = indexmap::map::IterMut<'a, String, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T> GenericMediaTrackConstraintSet<T> {
    pub fn iter(&self) -> indexmap::map::Iter<'_, String, T> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> indexmap::map::IterMut<'_, String, T> {
        self.0.iter_mut()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get<Q>(&self, property: Q) -> Option<&T>
    where
        Q: AsRef<str>,
    {
        self.0.get(property.as_ref())
    }

    pub fn insert<Q>(&mut self, property: Q, setting: T) -> Option<T>
    where
        Q: Into<String>,
    {
        self.0.insert(property.into(), setting)
    }

    /// Computes in **O(n)** time (average).
    pub fn remove<Q>(&mut self, property: Q) -> Option<T>
    where
        Q: AsRef<str>,
    {
        self.0.shift_remove(property.as_ref())
    }

    pub fn contains_key<Q>(&mut self, property: Q) -> bool
    where
        Q: AsRef<str>,
    {
        self.0.contains_key(property.as_ref())
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
