use std::{
    collections::HashMap,
    iter::FromIterator,
    ops::{Deref, DerefMut},
};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{MediaTrackProperty, MediaTrackSetting};

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
pub struct MediaTrackSettings(HashMap<MediaTrackProperty, MediaTrackSetting>);

impl MediaTrackSettings {
    /// Creates a settings value from its inner hashmap.
    pub fn new(settings: HashMap<MediaTrackProperty, MediaTrackSetting>) -> Self {
        Self(settings)
    }

    /// Consumes the value, returning its inner hashmap.
    pub fn into_inner(self) -> HashMap<MediaTrackProperty, MediaTrackSetting> {
        self.0
    }
}

impl Deref for MediaTrackSettings {
    type Target = HashMap<MediaTrackProperty, MediaTrackSetting>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MediaTrackSettings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> FromIterator<(T, MediaTrackSetting)> for MediaTrackSettings
where
    T: Into<MediaTrackProperty>,
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (T, MediaTrackSetting)>,
    {
        Self::new(iter.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

impl IntoIterator for MediaTrackSettings {
    type Item = (MediaTrackProperty, MediaTrackSetting);
    type IntoIter = std::collections::hash_map::IntoIter<MediaTrackProperty, MediaTrackSetting>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use crate::property::all::name::*;

    use super::*;

    type Subject = MediaTrackSettings;

    #[test]
    fn into_inner() {
        let hash_map = HashMap::from_iter([
            (DEVICE_ID.clone(), "device-id".into()),
            (AUTO_GAIN_CONTROL.clone(), true.into()),
            (CHANNEL_COUNT.clone(), 20.into()),
            (LATENCY.clone(), 2.0.into()),
        ]);

        let subject = Subject::new(hash_map.clone());

        let actual = subject.into_inner();

        let expected = hash_map;

        assert_eq!(actual, expected);
    }

    #[test]
    fn into_iter() {
        let hash_map = HashMap::from_iter([
            (DEVICE_ID.clone(), "device-id".into()),
            (AUTO_GAIN_CONTROL.clone(), true.into()),
            (CHANNEL_COUNT.clone(), 20.into()),
            (LATENCY.clone(), 2.0.into()),
        ]);

        let subject = Subject::new(hash_map.clone());

        let actual: HashMap<_, _> = subject.into_iter().collect();

        let expected = hash_map;

        assert_eq!(actual, expected);
    }

    #[test]
    fn deref_and_deref_mut() {
        let mut subject = Subject::default();

        // Deref mut:
        subject.insert(DEVICE_ID.clone(), "device-id".into());

        // Deref:
        assert!(subject.contains_key(&DEVICE_ID));
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::{macros::test_serde_symmetry, property::all::name::*};

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
            (&DEVICE_ID, "device-id".into()),
            (&AUTO_GAIN_CONTROL, true.into()),
            (&CHANNEL_COUNT, 2.into()),
            (&LATENCY, 0.123.into()),
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
