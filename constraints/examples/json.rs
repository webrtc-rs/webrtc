use std::iter::FromIterator;

use webrtc_constraints::{
    algorithms::{
        select_settings_candidates, ClosestToIdealPolicy, DeviceInformationExposureMode,
        TieBreakingPolicy,
    },
    property::all::name::*,
    MediaTrackConstraints, MediaTrackSettings, MediaTrackSupportedConstraints,
};

fn main() {
    let supported_constraints =
        MediaTrackSupportedConstraints::from_iter(vec![&DEVICE_ID, &HEIGHT, &WIDTH, &RESIZE_MODE]);

    // Deserialize possible settings from JSON:
    let possible_settings: Vec<MediaTrackSettings> = {
        let json = serde_json::json!([
            { "deviceId": "480p", "width": 720, "height": 480, "resizeMode": "crop-and-scale" },
            { "deviceId": "720p", "width": 1280, "height": 720, "resizeMode": "crop-and-scale" },
            { "deviceId": "1080p", "width": 1920, "height": 1080, "resizeMode": "none" },
            { "deviceId": "1440p", "width": 2560, "height": 1440, "resizeMode": "none" },
            { "deviceId": "2160p", "width": 3840, "height": 2160, "resizeMode": "none" },
        ]);
        serde_json::from_value(json).unwrap()
    };

    // Deserialize constraints from JSON:
    let constraints: MediaTrackConstraints = {
        let json = serde_json::json!({
            "width": {
                "max": 2560,
            },
            "height": {
                "max": 1440,
            },
            // Unsupported constraint, which should thus get ignored:
            "frameRate": {
                "exact": 30.0
            },
            // Ideal resize-mode:
            "resizeMode": "none",
            "advanced": [
                // The first advanced constraint set of "exact 800p" does not match
                // any candidate and should thus get ignored by the algorithm:
                { "height": 800 },
                // The second advanced constraint set of "no resizing" does match
                // candidates and should thus be applied by the algorithm:
                { "resizeMode": "none" },
            ]
        });
        serde_json::from_value(json).unwrap()
    };

    // Resolve bare values to proper constraints:
    let resolved_constraints = constraints.into_resolved();

    // Sanitize constraints, removing empty and unsupported constraints:
    let sanitized_constraints = resolved_constraints.into_sanitized(&supported_constraints);

    let candidates = select_settings_candidates(
        &possible_settings,
        &sanitized_constraints,
        DeviceInformationExposureMode::Protected,
    )
    .unwrap();

    // Specify a tie-breaking policy
    //
    // A couple of basic policies are provided batteries-included,
    // but for more sophisticated needs you can implement your own `TieBreakingPolicy`:
    let tie_breaking_policy =
        ClosestToIdealPolicy::new(possible_settings[2].clone(), &supported_constraints);

    let actual = tie_breaking_policy.select_candidate(candidates);

    let expected = &possible_settings[2];

    assert_eq!(actual, expected);
}
