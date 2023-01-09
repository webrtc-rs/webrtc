use std::{
    collections::HashSet,
    iter::FromIterator,
    ops::{Deref, DerefMut},
};

#[cfg(feature = "serde")]
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeMap,
    Deserialize, Deserializer, Serialize, Serializer,
};

use crate::MediaTrackProperty;

/// The list of constraints recognized by a User Agent for controlling the
/// capabilities of a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaTrackSupportedConstraints`][media_track_supported_constraints]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// The W3C spec defines `MediaTrackSupportedConstraints` in terma of a dictionary,
/// which per the [WebIDL spec][webidl_spec] is an ordered map (e.g. [`IndexSet<K>`][index_set]).
/// Since the spec however does not make use of the order of items
/// in the map we use a simple `HashSet<K>`.
///
/// [hash_set]: https://doc.rust-lang.org/std/collections/struct.HashSet.html
/// [index_set]: https://docs.rs/indexmap/latest/indexmap/set/struct.IndexSet.html
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_supported_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatracksupportedconstraints
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams
/// [webidl_spec]: https://webidl.spec.whatwg.org/#idl-dictionaries
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MediaTrackSupportedConstraints(HashSet<MediaTrackProperty>);

impl MediaTrackSupportedConstraints {
    /// Creates a supported constraints value from its inner hashmap.
    pub fn new(properties: HashSet<MediaTrackProperty>) -> Self {
        Self(properties)
    }

    /// Consumes the value, returning its inner hashmap.
    pub fn into_inner(self) -> HashSet<MediaTrackProperty> {
        self.0
    }
}

impl Deref for MediaTrackSupportedConstraints {
    type Target = HashSet<MediaTrackProperty>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MediaTrackSupportedConstraints {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for MediaTrackSupportedConstraints {
    /// [Default values][default_values] as defined by the W3C specification.
    ///
    /// [default_values]: https://www.w3.org/TR/mediacapture-streams/#dictionary-mediatracksupportedconstraints-members
    fn default() -> Self {
        use crate::property::all::names as property_names;

        Self::from_iter(property_names().into_iter().cloned())
    }
}

impl<T> FromIterator<T> for MediaTrackSupportedConstraints
where
    T: Into<MediaTrackProperty>,
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        Self(iter.into_iter().map(|property| property.into()).collect())
    }
}

impl IntoIterator for MediaTrackSupportedConstraints {
    type Item = MediaTrackProperty;
    type IntoIter = std::collections::hash_set::IntoIter<MediaTrackProperty>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for MediaTrackSupportedConstraints {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(SerdeVisitor)
    }
}

#[cfg(feature = "serde")]
impl Serialize for MediaTrackSupportedConstraints {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for property in &self.0 {
            map.serialize_entry(property, &true)?;
        }
        map.end()
    }
}

#[cfg(feature = "serde")]
struct SerdeVisitor;

#[cfg(feature = "serde")]
impl<'de> Visitor<'de> for SerdeVisitor {
    type Value = MediaTrackSupportedConstraints;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an object with strings as keys and `true` as values")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut set = HashSet::with_capacity(access.size_hint().unwrap_or(0));
        while let Some((key, value)) = access.next_entry()? {
            if value {
                set.insert(key);
            }
        }
        Ok(MediaTrackSupportedConstraints(set))
    }
}

#[cfg(test)]
mod tests {
    use crate::property::all::name::*;

    use super::*;

    type Subject = MediaTrackSupportedConstraints;

    #[test]
    fn into_inner() {
        let hash_set = HashSet::from_iter([
            DEVICE_ID.clone(),
            AUTO_GAIN_CONTROL.clone(),
            CHANNEL_COUNT.clone(),
            LATENCY.clone(),
        ]);

        let subject = Subject::new(hash_set.clone());

        let actual = subject.into_inner();

        let expected = hash_set;

        assert_eq!(actual, expected);
    }

    #[test]
    fn into_iter() {
        let hash_set = HashSet::from_iter([
            DEVICE_ID.clone(),
            AUTO_GAIN_CONTROL.clone(),
            CHANNEL_COUNT.clone(),
            LATENCY.clone(),
        ]);

        let subject = Subject::new(hash_set.clone());

        let actual: HashSet<_, _> = subject.into_iter().collect();

        let expected = hash_set;

        assert_eq!(actual, expected);
    }

    #[test]
    fn deref_and_deref_mut() {
        let mut subject = Subject::default();

        // Deref mut:
        subject.insert(DEVICE_ID.clone());

        // Deref:
        assert!(subject.contains(&DEVICE_ID));
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::{macros::test_serde_symmetry, property::all::name::*};

    use super::*;

    type Subject = MediaTrackSupportedConstraints;

    #[test]
    fn default() {
        let subject = Subject::default();
        let json = serde_json::json!({
            "deviceId": true,
            "groupId": true,
            "autoGainControl": true,
            "channelCount": true,
            "echoCancellation": true,
            "latency": true,
            "noiseSuppression": true,
            "sampleRate": true,
            "sampleSize": true,
            "aspectRatio": true,
            "facingMode": true,
            "frameRate": true,
            "height": true,
            "width": true,
            "resizeMode": true,
        });

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn customized() {
        let subject = Subject::from_iter([
            &DEVICE_ID,
            &GROUP_ID,
            &AUTO_GAIN_CONTROL,
            &CHANNEL_COUNT,
            &ASPECT_RATIO,
            &FACING_MODE,
        ]);
        let json = serde_json::json!({
            "deviceId": true,
            "groupId": true,
            "autoGainControl": true,
            "channelCount": true,
            "aspectRatio": true,
            "facingMode": true
        });

        test_serde_symmetry!(subject: subject, json: json);
    }
}
