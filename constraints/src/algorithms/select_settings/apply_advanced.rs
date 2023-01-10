use crate::{
    algorithms::FitnessDistance, constraints::SanitizedAdvancedMediaTrackConstraints,
    MediaTrackSettings,
};

/// Returns the set of settings for which all non-overconstraining advanced constraints'
/// fitness distance is finite.
///
/// Implements step 5 of the `SelectSettings` algorithm:
/// <https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings>
///
/// # Note:
/// This may change the order of items in `feasible_candidates`.
/// In practice however this is not a problem as we have to sort
/// it by fitness-distance eventually anyway.
pub(super) fn apply_advanced_constraints<'a>(
    mut candidates: Vec<(&'a MediaTrackSettings, f64)>,
    advanced_constraints: &SanitizedAdvancedMediaTrackConstraints,
) -> Vec<(&'a MediaTrackSettings, f64)> {
    // As specified in step 5 of the `SelectSettings` algorithm:
    // <https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings>
    //
    // > Iterate over the 'advanced' ConstraintSets in newConstraints in the order in which they were specified.
    // >
    // > For each ConstraintSet:
    // >
    // > 1. compute the fitness distance between it and each settings dictionary in candidates,
    // >    treating bare values of properties as exact.
    // >
    // > 2. If the fitness distance is finite for one or more settings dictionaries in candidates,
    // >    keep those settings dictionaries in candidates, discarding others.
    // >
    // >    If the fitness distance is infinite for all settings dictionaries in candidates,
    // >    ignore this ConstraintSet.

    let mut selected_candidates = Vec::with_capacity(candidates.len());

    // Double-buffered sieving to avoid excessive vec allocations:
    for advanced_constraint_set in advanced_constraints.iter() {
        for (candidate, fitness_distance) in candidates.iter() {
            if advanced_constraint_set.fitness_distance(candidate).is_ok() {
                selected_candidates.push((*candidate, *fitness_distance));
            }
        }

        if !selected_candidates.is_empty() {
            candidates.clear();
            std::mem::swap(&mut candidates, &mut selected_candidates);
        }
    }

    candidates
}

#[cfg(test)]
mod tests {
    use std::iter::FromIterator;

    use crate::{
        property::all::name::*, MediaTrackSupportedConstraints, ResizeMode,
        ResolvedAdvancedMediaTrackConstraints, ResolvedMediaTrackConstraintSet,
        ResolvedValueConstraint, ResolvedValueRangeConstraint,
    };

    use super::*;

    // Advanced constraint sets that doe not match any
    // candidates should just get ignored:
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

        let candidates: Vec<_> = settings
            .iter()
            // attach a dummy fitness function:
            .map(|settings| (settings, 42.0))
            .collect();

        let constraints = ResolvedAdvancedMediaTrackConstraints::from_iter([
            ResolvedMediaTrackConstraintSet::from_iter([(
                &DEVICE_ID,
                ResolvedValueConstraint::default()
                    .exact("bazblee".to_owned())
                    .into(),
            )]),
        ]);

        let sanitized_constraints = constraints.to_sanitized(&supported_constraints);

        let actual: Vec<_> = apply_advanced_constraints(candidates, &sanitized_constraints)
            .into_iter()
            // drop the dummy fitness distance:
            .map(|(settings, _)| settings)
            .collect();

        let expected: Vec<_> = settings.iter().collect();

        assert_eq!(actual, expected);
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

        let candidates: Vec<_> = settings.iter().map(|settings| (settings, 42.0)).collect();

        let constraints = ResolvedAdvancedMediaTrackConstraints::from_iter([
            // The first advanced constraint set of "exact 800p" does not match
            // any candidate and should thus get ignored by the algorithm:
            ResolvedMediaTrackConstraintSet::from_iter([(
                &HEIGHT,
                ResolvedValueRangeConstraint::default().exact(800).into(),
            )]),
            // The second advanced constraint set of "no resizing" does match
            // candidates and should thus be applied by the algorithm:
            ResolvedMediaTrackConstraintSet::from_iter([(
                &RESIZE_MODE,
                ResolvedValueConstraint::default()
                    .exact(ResizeMode::none())
                    .into(),
            )]),
            // The second advanced constraint set of "max 1440p" does match
            // candidates and should thus be applied by the algorithm:
            ResolvedMediaTrackConstraintSet::from_iter([(
                &HEIGHT,
                ResolvedValueRangeConstraint::default().max(1440).into(),
            )]),
        ]);

        let sanitized_constraints = constraints.to_sanitized(&supported_constraints);

        let actual: Vec<_> = apply_advanced_constraints(candidates, &sanitized_constraints)
            .into_iter()
            // drop the dummy fitness distance:
            .map(|(settings, _)| settings)
            .collect();

        let expected = vec![&settings[2], &settings[3]];

        assert_eq!(actual, expected);
    }
}
