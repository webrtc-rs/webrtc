use std::collections::HashMap;

use crate::{
    algorithms::{
        select_settings::{ConstraintFailureInfo, DeviceInformationExposureMode},
        FitnessDistance,
    },
    errors::OverconstrainedError,
    MediaTrackProperty, MediaTrackSettings, SanitizedMediaTrackConstraintSet,
};

/// Returns the set of settings for which all mandatory constraints'
/// fitness distance is finite.
///
/// Implements step 5 of the `SelectSettings` algorithm:
/// <https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings>
pub(super) fn apply_mandatory_constraints<'a, I>(
    candidates: I,
    mandatory_constraints: &SanitizedMediaTrackConstraintSet,
    exposure_mode: DeviceInformationExposureMode,
) -> Result<Vec<(&'a MediaTrackSettings, f64)>, OverconstrainedError>
where
    I: IntoIterator<Item = &'a MediaTrackSettings>,
{
    // As specified in step 3 of the `SelectSettings` algorithm:
    // <https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings>
    //
    // > For every possible settings dictionary of copy compute its fitness distance,
    // > treating bare values of properties as ideal values. Let candidates be the
    // > set of settings dictionaries for which the fitness distance is finite.

    let mut feasible_candidates: Vec<(&'a MediaTrackSettings, f64)> = vec![];
    let mut failed_constraints: HashMap<MediaTrackProperty, ConstraintFailureInfo> =
        Default::default();

    for candidate in candidates {
        match mandatory_constraints.fitness_distance(candidate) {
            Ok(fitness_distance) => {
                debug_assert!(fitness_distance.is_finite());

                feasible_candidates.push((candidate, fitness_distance));
            }
            Err(error) => {
                for (property, setting_error) in error.setting_errors {
                    let entry = failed_constraints
                        .entry(property)
                        .or_insert_with(Default::default);
                    entry.failures += 1;
                    entry.errors.insert(setting_error);
                }
            }
        }
    }

    if feasible_candidates.is_empty() {
        return Err(match exposure_mode {
            DeviceInformationExposureMode::Exposed => {
                OverconstrainedError::exposing_device_information(failed_constraints)
            }
            DeviceInformationExposureMode::Protected => OverconstrainedError::default(),
        });
    }

    Ok(feasible_candidates)
}

#[cfg(test)]
mod tests {
    use std::iter::FromIterator;

    use crate::{
        property::all::name::*, MediaTrackSupportedConstraints, ResizeMode,
        ResolvedMandatoryMediaTrackConstraints, ResolvedValueConstraint,
        ResolvedValueRangeConstraint,
    };

    use super::*;

    // Advanced constraint sets that do not match any candidates should just get ignored:
    #[test]
    fn overconstrained() {
        let supported_constraints = MediaTrackSupportedConstraints::from_iter(vec![
            &DEVICE_ID,
            &HEIGHT,
            &WIDTH,
            &RESIZE_MODE,
        ]);

        let settings = vec![
            MediaTrackSettings::from_iter([(&DEVICE_ID, "foo".into())]),
            MediaTrackSettings::from_iter([(&DEVICE_ID, "bar".into())]),
        ];

        let candidates: Vec<_> = settings.iter().collect();

        let constraints = ResolvedMandatoryMediaTrackConstraints::from_iter([(
            &DEVICE_ID,
            ResolvedValueConstraint::default()
                .exact("mismatched-device".to_owned())
                .into(),
        )]);

        let sanitized_constraints = constraints.to_sanitized(&supported_constraints);

        // Exposed exposure mode:

        let error = apply_mandatory_constraints(
            candidates.clone(),
            &sanitized_constraints,
            DeviceInformationExposureMode::Exposed,
        )
        .unwrap_err();

        let constraint = &error.constraint;
        let err_message = error.message.as_ref().expect("Error message.");

        assert_eq!(constraint, &DEVICE_ID);
        assert_eq!(
            err_message,
            "Setting was a mismatch ([\"bar\", \"foo\"] do not satisfy (x == \"mismatched-device\"))."
        );

        // Protected exposure mode:

        let error = apply_mandatory_constraints(
            candidates,
            &sanitized_constraints,
            DeviceInformationExposureMode::Protected,
        )
        .unwrap_err();

        let constraint = &error.constraint;
        let err_message = error.message;

        assert_eq!(
            constraint,
            &MediaTrackProperty::from(""),
            "Constraint should not have been exposed"
        );
        assert!(
            err_message.is_none(),
            "Error message should not have been exposed"
        );
    }

    #[test]
    fn constrained() {
        let supported_constraints = MediaTrackSupportedConstraints::from_iter(vec![
            &DEVICE_ID,
            &HEIGHT,
            &WIDTH,
            &RESIZE_MODE,
        ]);

        let settings = vec![
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

        let candidates: Vec<_> = settings.iter().collect();

        let constraints = ResolvedMandatoryMediaTrackConstraints::from_iter([
            (
                &RESIZE_MODE,
                ResolvedValueConstraint::default()
                    .exact(ResizeMode::none())
                    .into(),
            ),
            (
                &HEIGHT,
                ResolvedValueRangeConstraint::default().min(1000).into(),
            ),
            (
                &WIDTH,
                ResolvedValueRangeConstraint::default().max(2000).into(),
            ),
        ]);

        let sanitized_constraints = constraints.to_sanitized(&supported_constraints);

        let actual = apply_mandatory_constraints(
            candidates,
            &sanitized_constraints,
            DeviceInformationExposureMode::Exposed,
        )
        .unwrap();

        let expected = vec![(&settings[2], 0.0)];

        assert_eq!(actual, expected);
    }
}
