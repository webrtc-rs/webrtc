#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    BareOrMediaTrackConstraint, MediaTrackConstraint,
};

use super::track::GenericBoolOrMediaTrackConstraints;

/// The constraints for a [`MediaStream`][media_stream] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaStreamConstraints`][media_stream_constraints]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// Unlike `MediaStreamConstraints` this type does not contain constraints
/// with bare values, but has them resolved to full constraints instead.
///
/// [media_stream]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastream
/// [media_stream_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamconstraints
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams/
pub type BareOrMediaStreamConstraints = GenericMediaStreamConstraints<BareOrMediaTrackConstraint>;

/// The constraints for a [`MediaStream`][media_stream] object.
///
/// # W3C Spec Compliance
///
/// Corresponds to [`MediaStreamConstraints`][media_stream_constraints]
/// from the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// Unlike `BareOrMediaStreamConstraints` this type may contain constraints with bare values.
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
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct GenericMediaStreamConstraints<T> {
    #[cfg_attr(feature = "serde", serde(default))]
    pub audio: GenericBoolOrMediaTrackConstraints<T>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub video: GenericBoolOrMediaTrackConstraints<T>,
}

impl BareOrMediaStreamConstraints {
    pub fn to_resolved(&self) -> MediaStreamConstraints {
        self.clone().into_resolved()
    }

    pub fn into_resolved(self) -> MediaStreamConstraints {
        let Self { audio, video } = self;
        MediaStreamConstraints {
            audio: audio.into_resolved(),
            video: video.into_resolved(),
        }
    }
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod tests {
    use crate::{
        constraints::{
            advanced::BareOrAdvancedMediaTrackConstraints, track::BareOrMediaTrackConstraints,
        },
        macros::test_serde_symmetry,
        property::name::*,
        BareOrBoolOrMediaTrackConstraints, BareOrMediaTrackConstraintSet,
    };

    use super::*;

    type Subject = BareOrMediaStreamConstraints;

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
            audio: BareOrBoolOrMediaTrackConstraints::Constraints(BareOrMediaTrackConstraints {
                basic_or_required: BareOrMediaTrackConstraintSet::from_iter([
                    (DEVICE_ID, "microphone".into()),
                    (CHANNEL_COUNT, 2.into()),
                ]),
                advanced: BareOrAdvancedMediaTrackConstraints::new(vec![
                    BareOrMediaTrackConstraintSet::from_iter([(LATENCY, 0.123.into())]),
                ]),
            }),
            video: BareOrBoolOrMediaTrackConstraints::Bool(true),
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
