use std::iter::FromIterator;

use webrtc_constraints::{
    algorithms::{
        select_settings_candidates, ClosestToIdealPolicy, DeviceInformationExposureMode,
        TieBreakingPolicy,
    },
    macros::*,
    property::all::name::*,
    settings, MediaTrackSupportedConstraints, ResizeMode,
};

fn main() {
    let supported_constraints =
        MediaTrackSupportedConstraints::from_iter(vec![&DEVICE_ID, &HEIGHT, &WIDTH, &RESIZE_MODE]);

    let possible_settings = vec![
        settings![
            &DEVICE_ID => "480p",
            &HEIGHT => 480,
            &WIDTH => 720,
            &RESIZE_MODE => ResizeMode::crop_and_scale(),
        ],
        settings![
            &DEVICE_ID => "720p",
            &HEIGHT => 720,
            &WIDTH => 1280,
            &RESIZE_MODE => ResizeMode::crop_and_scale(),
        ],
        settings![
            &DEVICE_ID => "1080p",
            &HEIGHT => 1080,
            &WIDTH => 1920,
            &RESIZE_MODE => ResizeMode::none(),
        ],
        settings![
            &DEVICE_ID => "1440p",
            &HEIGHT => 1440,
            &WIDTH => 2560,
            &RESIZE_MODE => ResizeMode::none(),
        ],
        settings![
            &DEVICE_ID => "2160p",
            &HEIGHT => 2160,
            &WIDTH => 3840,
            &RESIZE_MODE => ResizeMode::none(),
        ],
    ];

    let constraints = constraints! {
        mandatory: {
            &WIDTH => value_range_constraint!{
                max: 2560
            },
            &HEIGHT => value_range_constraint!{
                max: 1440
            },
            // Unsupported constraint, which should thus get ignored:
            &FRAME_RATE => value_range_constraint!{
                exact: 30.0
            },
        },
        advanced: [
            // The first advanced constraint set of "exact 800p" does not match
            // any candidate and should thus get ignored by the algorithm:
            {
                &HEIGHT => value_range_constraint!{
                    exact: 800
                }
            },
            // The second advanced constraint set of "no resizing" does match
            // candidates and should thus be applied by the algorithm:
            {
                &RESIZE_MODE => value_constraint!{
                    exact: ResizeMode::none()
                }
            },
        ]
    };

    // Resolve bare values to proper constraints:
    let resolved_constraints = constraints.into_resolved();

    // Sanitize constraints, removing empty and unsupported constraints:
    let sanitized_constraints = resolved_constraints.to_sanitized(&supported_constraints);

    let candidates = select_settings_candidates(
        &possible_settings,
        &sanitized_constraints,
        DeviceInformationExposureMode::Exposed,
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
