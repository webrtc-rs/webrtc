use std::iter::FromIterator;

use webrtc_constraints::{
    algorithms::{
        select_settings_candidates, ClosestToIdealPolicy, DeviceInformationExposureMode,
        TieBreakingPolicy,
    },
    property::all::name::*,
    AdvancedMediaTrackConstraints, MandatoryMediaTrackConstraints, MediaTrackConstraintSet,
    MediaTrackConstraints, MediaTrackSettings, MediaTrackSupportedConstraints, ResizeMode,
    ResolvedValueConstraint, ResolvedValueRangeConstraint, ValueConstraint, ValueRangeConstraint,
};

fn main() {
    let supported_constraints =
        MediaTrackSupportedConstraints::from_iter(vec![&DEVICE_ID, &HEIGHT, &WIDTH, &RESIZE_MODE]);

    let possible_settings = vec![
        MediaTrackSettings::from_iter([
            (&DEVICE_ID, "480p".into()),
            (&HEIGHT, 480.into()),
            (&WIDTH, 720.into()),
            (&RESIZE_MODE, ResizeMode::crop_and_scale().into()),
        ]),
        MediaTrackSettings::from_iter([
            (&DEVICE_ID, "720p".into()),
            (&HEIGHT, 720.into()),
            (&WIDTH, 1280.into()),
            (&RESIZE_MODE, ResizeMode::crop_and_scale().into()),
        ]),
        MediaTrackSettings::from_iter([
            (&DEVICE_ID, "1080p".into()),
            (&HEIGHT, 1080.into()),
            (&WIDTH, 1920.into()),
            (&RESIZE_MODE, ResizeMode::none().into()),
        ]),
        MediaTrackSettings::from_iter([
            (&DEVICE_ID, "1440p".into()),
            (&HEIGHT, 1440.into()),
            (&WIDTH, 2560.into()),
            (&RESIZE_MODE, ResizeMode::none().into()),
        ]),
        MediaTrackSettings::from_iter([
            (&DEVICE_ID, "2160p".into()),
            (&HEIGHT, 2160.into()),
            (&WIDTH, 3840.into()),
            (&RESIZE_MODE, ResizeMode::none().into()),
        ]),
    ];

    let constraints = MediaTrackConstraints {
        mandatory: MandatoryMediaTrackConstraints::from_iter([
            (
                &WIDTH,
                ValueRangeConstraint::Constraint(ResolvedValueRangeConstraint::default().max(2560))
                    .into(),
            ),
            (
                &HEIGHT,
                ValueRangeConstraint::Constraint(ResolvedValueRangeConstraint::default().max(1440))
                    .into(),
            ),
            // Unsupported constraint, which should thus get ignored:
            (
                &FRAME_RATE,
                ValueRangeConstraint::Constraint(
                    ResolvedValueRangeConstraint::default().exact(30.0),
                )
                .into(),
            ),
        ]),
        advanced: AdvancedMediaTrackConstraints::from_iter([
            // The first advanced constraint set of "exact 800p" does not match
            // any candidate and should thus get ignored by the algorithm:
            MediaTrackConstraintSet::from_iter([(
                &HEIGHT,
                ValueRangeConstraint::Constraint(
                    ResolvedValueRangeConstraint::default().exact(800),
                )
                .into(),
            )]),
            // The second advanced constraint set of "no resizing" does match
            // candidates and should thus be applied by the algorithm:
            MediaTrackConstraintSet::from_iter([(
                &RESIZE_MODE,
                ValueConstraint::Constraint(
                    ResolvedValueConstraint::default().exact(ResizeMode::none()),
                )
                .into(),
            )]),
        ]),
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
