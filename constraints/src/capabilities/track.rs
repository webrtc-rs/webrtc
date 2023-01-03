use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

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

    pub fn into_inner(self) -> HashMap<String, MediaTrackCapability> {
        self.0
    }
}

impl Deref for MediaTrackCapabilities {
    type Target = HashMap<String, MediaTrackCapability>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MediaTrackCapabilities {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
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

#[cfg(feature = "serde")]
#[cfg(test)]
mod serde_tests {
    use crate::{macros::test_serde_symmetry, property::all::name::*};

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
