#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MediaTrackConstraint;

use super::track::GenericBoolOrMediaTrackConstraints;

/// The constraints for a [`MediaStream`][media_stream] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaStreamConstraints`][media_stream_constraints]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastream
/// [media_stream_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
pub type MediaStreamConstraints = GenericMediaStreamConstraints<MediaTrackConstraint>;

/// The constraints for a [`MediaStream`][media_stream] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaStreamConstraints`][media_stream_constraints]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastream
/// [media_stream_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
#[derive(Debug, Clone, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct GenericMediaStreamConstraints<T> {
    #[cfg_attr(feature = "serde", serde(default))]
    pub audio: GenericBoolOrMediaTrackConstraints<T>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub video: GenericBoolOrMediaTrackConstraints<T>,
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod tests {
    use std::iter::FromIterator;

    use crate::{
        constraints::{
            advanced::AdvancedMediaTrackConstraints,
            mandatory::MandatoryMediaTrackConstraints,
            track::{BoolOrMediaTrackConstraints, MediaTrackConstraints},
        },
        macros::test_serde_symmetry,
        property::all::name::*,
        MediaTrackConstraintSet,
    };

    use super::*;

    type Subject = MediaStreamConstraints;

    #[test]
    fn default() {
        let subject = Subject::default();
        let json = serde_json::json!({
            "audio": false,
            "video": false,
        });
        test_serde_symmetry!(subject: subject, json: json);
    }

    #[test]
    fn customized() {
        let subject = Subject {
            audio: BoolOrMediaTrackConstraints::Constraints(MediaTrackConstraints {
                mandatory: MandatoryMediaTrackConstraints::from_iter([
                    (&DEVICE_ID, "microphone".into()),
                    (&CHANNEL_COUNT, 2.into()),
                ]),
                advanced: AdvancedMediaTrackConstraints::new(vec![
                    MediaTrackConstraintSet::from_iter([(&LATENCY, 0.123.into())]),
                ]),
            }),
            video: BoolOrMediaTrackConstraints::Bool(true),
        };
        let json = serde_json::json!({
            "audio": {
                "deviceId": "microphone",
                "channelCount": 2_i64,
                "advanced": [
                    { "latency": 0.123_f64, }
                ]
            },
            "video": true,
        });
        test_serde_symmetry!(subject: subject, json: json);
    }
}
