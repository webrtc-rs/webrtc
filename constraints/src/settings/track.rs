use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackSetting;

/// The settings of a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaTrackSettings`][media_track_settings]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// The W3C spec defines `MediaTrackSettings` in terma of a dictionary,
/// which per the [WebIDL spec][webidl_spec] is an ordered map (e.g. [`IndexMap<K, V>`][index_map]).
/// Since the spec however does not make use of the order of items
/// in the map we use a simple [`HashMap<K>`][hash_map].
///
/// [hash_map]: https://doc.rust-lang.org/std/collections/struct.HashMap.html
/// [index_map]: https://docs.rs/indexmap/latest/indexmap/set/struct.IndexMap.html
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_settings]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatracksettings
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams
/// [webidl_spec]: https://webidl.spec.whatwg.org/#idl-dictionaries
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct MediaTrackSettings(HashMap<String, MediaTrackSetting>);

impl MediaTrackSettings {
    pub fn new(settings: HashMap<String, MediaTrackSetting>) -> Self {
        Self(settings)
    }
}

impl<T> FromIterator<(T, MediaTrackSetting)> for MediaTrackSettings
where
    T: Into<String>,
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (T, MediaTrackSetting)>,
    {
        Self::new(iter.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

impl IntoIterator for MediaTrackSettings {
    type Item = (String, MediaTrackSetting);
    type IntoIter = std::collections::hash_map::IntoIter<String, MediaTrackSetting>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a MediaTrackSettings {
    type Item = (&'a String, &'a MediaTrackSetting);
    type IntoIter = std::collections::hash_map::Iter<'a, String, MediaTrackSetting>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut MediaTrackSettings {
    type Item = (&'a String, &'a mut MediaTrackSetting);
    type IntoIter = std::collections::hash_map::IterMut<'a, String, MediaTrackSetting>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl MediaTrackSettings {
    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, String, MediaTrackSetting> {
        self.0.iter()
    }

    pub fn iter_mut(
        &mut self,
    ) -> std::collections::hash_map::IterMut<'_, String, MediaTrackSetting> {
        self.0.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get<T>(&self, property: T) -> Option<&MediaTrackSetting>
    where
        T: AsRef<str>,
    {
        self.0.get(property.as_ref())
    }

    pub fn insert<T>(
        &mut self,
        property: T,
        setting: MediaTrackSetting,
    ) -> Option<MediaTrackSetting>
    where
        T: Into<String>,
    {
        self.0.insert(property.into(), setting)
    }

    pub fn remove<T>(&mut self, property: T) -> Option<MediaTrackSetting>
    where
        T: AsRef<str>,
    {
        self.0.remove(property.as_ref())
    }

    pub fn contains_key<T>(&mut self, property: T) -> bool
    where
        T: AsRef<str>,
    {
        self.0.contains_key(property.as_ref())
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::{macros::test_serde_symmetry, property::name::*};

    use super::*;

    type Subject = MediaTrackSettings;

    #[test]
    fn default() {
        let subject = Subject::default();
        let json = serde_json::json!({});

        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn customized() {
        let subject = Subject::from_iter([
            (DEVICE_ID, "device-id".into()),
            (AUTO_GAIN_CONTROL, true.into()),
            (CHANNEL_COUNT, 2.into()),
            (LATENCY, 0.123.into()),
        ]);
        let json = serde_json::json!({
            "deviceId": "device-id".to_owned(),
            "autoGainControl": true,
            "channelCount": 2,
            "latency": 0.123,
        });

        test_serde_symmetry!(subject: subject, json: json);
    }
}
