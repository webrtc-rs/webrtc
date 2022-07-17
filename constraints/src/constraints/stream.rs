#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::BoolOrMediaTrackConstraints;

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
pub struct MediaStreamConstraints {
    #[cfg_attr(feature = "serde", serde(default))]
    pub audio: BoolOrMediaTrackConstraints,
    #[cfg_attr(feature = "serde", serde(default))]
    pub video: BoolOrMediaTrackConstraints,
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod tests {
    use crate::{
        constraints::advanced::BareOrAdvancedMediaTrackConstraints, macros::test_serde_symmetry,
        property::name::*, BareOrMediaTrackConstraintSet, MediaTrackConstraints,
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
                basic: BareOrMediaTrackConstraintSet::from_iter([
                    (DEVICE_ID, "microphone".into()),
                    (CHANNEL_COUNT, 2.into()),
                ]),
                advanced: BareOrAdvancedMediaTrackConstraints::new(vec![
                    BareOrMediaTrackConstraintSet::from_iter([(LATENCY, 0.123.into())]),
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
