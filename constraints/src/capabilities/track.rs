use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackCapability;

/// The capabilities of a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaTrackCapabilities`][media_track_capabilities]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// The W3C spec defines `MediaTrackSettings` in terma of a dictionary,
/// which per the [WebIDL spec][webidl_spec] is an ordered map (e.g. `IndexMap<K, V>`).
/// Since the spec however does not make use of the order of items
/// in the map we use a simple `HashMap<K, V>`.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_capabilities]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatrackcapabilities
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams
/// [webidl_spec]: https://webidl.spec.whatwg.org/#idl-dictionaries
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct MediaTrackCapabilities(HashMap<String, MediaTrackCapability>);

impl MediaTrackCapabilities {
    pub fn new(capabilities: HashMap<String, MediaTrackCapability>) -> Self {
        Self(capabilities)
    }
}

impl<T> FromIterator<(T, MediaTrackCapability)> for MediaTrackCapabilities
where
    T: Into<String>,
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (T, MediaTrackCapability)>,
    {
        Self::new(iter.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

impl IntoIterator for MediaTrackCapabilities {
    type Item = (String, MediaTrackCapability);
    type IntoIter = std::collections::hash_map::IntoIter<String, MediaTrackCapability>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a MediaTrackCapabilities {
    type Item = (&'a String, &'a MediaTrackCapability);
    type IntoIter = std::collections::hash_map::Iter<'a, String, MediaTrackCapability>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut MediaTrackCapabilities {
    type Item = (&'a String, &'a mut MediaTrackCapability);
    type IntoIter = std::collections::hash_map::IterMut<'a, String, MediaTrackCapability>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl MediaTrackCapabilities {
    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, String, MediaTrackCapability> {
        self.0.iter()
    }

    pub fn iter_mut(
        &mut self,
    ) -> std::collections::hash_map::IterMut<'_, String, MediaTrackCapability> {
        self.0.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn insert<T>(&mut self, property: T, capability: MediaTrackCapability)
    where
        T: Into<String>,
    {
        self.0.insert(property.into(), capability);
    }

    pub fn remove<T>(&mut self, property: T) -> Option<MediaTrackCapability>
    where
        T: AsRef<str>,
    {
        self.0.remove(property.as_ref())
    }

    pub fn contains<T>(&mut self, property: T) -> bool
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

    type Subject = MediaTrackCapabilities;

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
            (CHANNEL_COUNT, (12..=34).into()),
            (LATENCY, (1.2..=3.4).into()),
        ]);
        let json = serde_json::json!({
            "deviceId": "device-id".to_owned(),
            "autoGainControl": true,
            "channelCount": { "min": 12, "max": 34 },
            "latency": { "min": 1.2, "max": 3.4 },
        });

        test_serde_symmetry!(subject: subject, json: json);
    }
}
