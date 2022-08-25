use std::collections::{HashMap, HashSet};

use crate::{
    algorithms::{fitness_distance::SettingFitnessDistanceError, FitnessDistance},
    errors::OverconstrainedError,
    MediaTrackSettings, SanitizedMandatoryMediaTrackConstraints, SanitizedMediaTrackConstraintSet,
    SanitizedMediaTrackConstraints,
};

pub trait SelectSettingsPolicy {
    /// Selects a preferred candidate from a non-empty selection of optimal candidates.
    ///
    /// As specified in step 5 of the `SelectSettings` algorithm:
    /// https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings
    ///
    /// > Select one settings dictionary from candidates, and return it as the result
    /// > of the SelectSettings algorithm. The User Agent MUST use one with the
    /// > smallest fitness distance, as calculated in step 3.
    /// > If more than one settings dictionary have the smallest fitness distance,
    /// > the User Agent chooses one of them based on system default property values
    /// > and User Agent default property values.
    ///
    /// The default implementation picks the first.
    fn select_candidate<I>(&self, mut candidates: I) -> MediaTrackSettings
    where
        I: Iterator<Item = MediaTrackSettings>,
    {
        // Safety: We know that `candidates is non-empty:
        candidates
            .next()
            .expect("The `candidates` iterator must produce at least one item.")
    }
}

pub enum SelectSettingsError {
    Overconstrained(OverconstrainedError),
}

impl From<OverconstrainedError> for SelectSettingsError {
    fn from(error: OverconstrainedError) -> Self {
        Self::Overconstrained(error)
    }
}

pub fn select_settings<I, P>(
    possible_settings: I,
    constraints: &SanitizedMediaTrackConstraints,
    policy: &P,
) -> Result<MediaTrackSettings, SelectSettingsError>
where
    I: Iterator<Item = MediaTrackSettings>,
    P: SelectSettingsPolicy,
{
    // As specified in step 1 of the `SelectSettings` algorithm:
    // https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings
    //
    // > Each constraint specifies one or more values (or a range of values) for its property.
    // > A property MAY appear more than once in the list of 'advanced' ConstraintSets.
    // > If an empty list has been given as the value for a constraint,
    // > it MUST be interpreted as if the constraint were not specified
    // > (in other words, an empty constraint == no constraint).
    // >
    // > Note that unknown properties are discarded by WebIDL,
    // > which means that unknown/unsupported required constraints will silently disappear.
    // > To avoid this being a surprise, application authors are expected to first use
    // > the `getSupportedConstraints()` method […].

    // Obtain candidates by filtering possible settings, dropping those with infinite fitness distances:
    //
    // This function call corresponds to steps 3 & 4 of the `SelectSettings` algorithm:
    // https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings

    let mut candidates = evaluate_candidates(possible_settings, &constraints.mandatory)?;

    // As specified in step 5 of the `SelectSettings` algorithm:
    // https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings
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
    for advanced_constraint in &*constraints.advanced {
        let results: Vec<bool> = candidates
            .iter()
            .map(
                |(candidate, _)| match advanced_constraint.fitness_distance(candidate) {
                    Ok(fitness_distance) => {
                        debug_assert!(fitness_distance.is_finite());
                        true
                    }
                    Err(_) => false,
                },
            )
            .collect();

        if !results.iter().any(|is_finite| *is_finite) {
            continue;
        }

        candidates = candidates
            .into_iter()
            .zip(results)
            .filter_map(
                |(candidate, is_finite)| {
                    if is_finite {
                        Some(candidate)
                    } else {
                        None
                    }
                },
            )
            .collect();
    }

    // Sort candidates by their fitness distance, as obtained in an earlier step:
    candidates.sort_by(|lhs, rhs| {
        // Safety: We know (since we check for it in `fitness_distance()`) that `candidates` does
        // not contain candidates with non-finite distances, so we can safely unwrap the comparison:
        lhs.1
            .partial_cmp(&rhs.1)
            .expect("Fitness distances should be finite at this point.")
    });

    // Safety:
    // - We know that `select_candidates()` returns a non-empty vec of candidates.
    // - We know that applying advanced constraint-sets will always keep at least one candidate.
    // As such we can safely unwrap the first element of the vec:

    let best_fitness_distance = candidates[0].1;

    let best_candidates = candidates
        .into_iter()
        .take_while(|(_, fitness_distance)| *fitness_distance == best_fitness_distance)
        .map(|(candidate, _)| candidate);

    // Select one settings value from candidates:
    //
    // This function call corresponds to step 5 of the `SelectSettings` algorithm:
    // https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings
    let best_candidate = policy.select_candidate(best_candidates);

    Ok(best_candidate)
}

fn evaluate_candidates<I>(
    possible_settings: I,
    constraints: &SanitizedMandatoryMediaTrackConstraints,
) -> Result<Vec<(MediaTrackSettings, f64)>, OverconstrainedError>
where
    I: Iterator<Item = MediaTrackSettings>,
{
    #[derive(Default)]
    struct FailureInfo {
        failures: usize,
        errors: HashSet<SettingFitnessDistanceError>,
    }

    let mut possible_settings_len = 0;
    let mut candidates: Vec<(MediaTrackSettings, f64)> = vec![];
    let mut failed_constraints: HashMap<String, FailureInfo> = Default::default();

    // As specified in step 3 of the `SelectSettings` algorithm:
    // https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings
    //
    // > For every possible settings dictionary of copy compute its fitness distance,
    // > treating bare values of properties as ideal values. Let candidates be the
    // > set of settings dictionaries for which the fitness distance is finite.

    for settings in possible_settings {
        possible_settings_len += 1;
        match constraints.fitness_distance(&settings) {
            Ok(fitness_distance) => {
                candidates.push((settings, fitness_distance));
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

    if candidates.is_empty() {
        // As specified in step 3 of the `SelectSettings` algorithm:
        // https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings
        //
        // > If `candidates` is empty, return `undefined` as the result of the `SelectSettings` algorithm.

        let failed_constraint = failed_constraints
            .into_iter()
            .max_by_key(|(_, failure_info)| failure_info.failures);

        let (constraint, failure_info) =
            failed_constraint.expect("Empty candidates implies non-empty failed constraints");

        if failure_info.failures == possible_settings_len {
            return Err(generate_overconstrained_error(
                constraint,
                failure_info.errors,
            ));
        }
    }

    Ok(candidates)
}

#[derive(Default)]
struct ConstraintFailureInfo {
    failures: usize,
    errors: HashSet<SettingFitnessDistanceError>,
}

fn select_feasible_candidates<I>(
    possible_settings: I,
    basic_or_required_constraints: &SanitizedMediaTrackConstraintSet,
) -> Result<Vec<(MediaTrackSettings, f64)>, OverconstrainedError>
where
    I: Iterator<Item = MediaTrackSettings>,
{
    let mut settings_len = 0;
    let mut candidates: Vec<(MediaTrackSettings, f64)> = vec![];
    let mut failed_constraints: HashMap<String, ConstraintFailureInfo> = Default::default();

    // As specified in step 3 of the `SelectSettings` algorithm:
    // https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings
    //
    // > For every possible settings dictionary of copy compute its fitness distance,
    // > treating bare values of properties as ideal values. Let candidates be the
    // > set of settings dictionaries for which the fitness distance is finite.

    for settings in possible_settings {
        settings_len += 1;
        match basic_or_required_constraints.fitness_distance(&settings) {
            Ok(fitness_distance) => {
                candidates.push((settings, fitness_distance));
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

    if candidates.is_empty() {
        let error = select_failed_constraint(settings_len, failed_constraints);
        return Err(error);
    }

    Ok(candidates)
}

fn select_failed_constraint(
    settings_len: usize,
    failed_constraints: HashMap<String, ConstraintFailureInfo>,
) -> OverconstrainedError {
    let failed_constraint = failed_constraints
        .into_iter()
        .max_by_key(|(_, failure_info)| failure_info.failures);

    let (constraint, failure_info) =
        failed_constraint.expect("Empty candidates implies non-empty failed constraints");

    if failure_info.failures == settings_len {
        generate_overconstrained_error(constraint, failure_info.errors)
    } else {
        OverconstrainedError::default()
    }
}

fn generate_overconstrained_error(
    constraint: String,
    errors: HashSet<SettingFitnessDistanceError>,
) -> OverconstrainedError {
    let formatted_reasons: Vec<_> = errors
        .into_iter()
        .map(|error| {
            match error {
                SettingFitnessDistanceError::Missing => "missing",
                SettingFitnessDistanceError::Mismatch => "a mismatch",
                SettingFitnessDistanceError::TooSmall => "too small",
                SettingFitnessDistanceError::TooLarge => "too large",
                SettingFitnessDistanceError::Invalid => "invalid",
            }
            .to_owned()
        })
        .collect();
    let formatted_reason = match &formatted_reasons[..] {
        [] => unreachable!(),
        [reason] => reason.clone(),
        [reasons @ .., reason] => {
            let reasons = reasons.join(", ");
            format!("either {}, or {}", reasons, reason)
        }
    };
    let message = Some(format!("The provided settings were {}.", formatted_reason));
    OverconstrainedError {
        constraint,
        message,
    }
}
