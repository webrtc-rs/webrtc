use std::collections::{HashMap, HashSet};

use ordered_float::NotNan;

use crate::{
    algorithms::{
        fitness_distance::SettingFitnessDistanceError, FitnessDistance,
        SettingFitnessDistanceErrorKind,
    },
    constraints::SanitizedAdvancedMediaTrackConstraints,
    errors::OverconstrainedError,
    BareOrMandatoryMediaTrackConstraints, MediaTrackSettings, MediaTrackSupportedConstraints,
    SanitizedMandatoryMediaTrackConstraints, SanitizedMediaTrackConstraintSet,
    SanitizedMediaTrackConstraints,
};

pub trait TieBreakingPolicy {
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
    fn select_candidate<'a, I>(&self, candidates: I) -> &'a MediaTrackSettings
    where
        I: Iterator<Item = &'a MediaTrackSettings>;
}

/// A naïve settings selection policy that just picks the first item of the iterator.
pub struct SelectFirstSettingsPolicy;

impl SelectFirstSettingsPolicy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SelectFirstSettingsPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl TieBreakingPolicy for SelectFirstSettingsPolicy {
    fn select_candidate<'a, I>(&self, mut candidates: I) -> &'a MediaTrackSettings
    where
        I: Iterator<Item = &'a MediaTrackSettings>,
    {
        // Safety: We know that `candidates is non-empty:
        candidates
            .next()
            .expect("The `candidates` iterator should have produced at least one item.")
    }
}

/// A settings selection policy that picks the item that's closest to the ideal.
pub struct SelectIdealSettingsPolicy {
    sanitized_constraints: SanitizedMandatoryMediaTrackConstraints,
}

impl SelectIdealSettingsPolicy {
    pub fn new(
        ideal_settings: MediaTrackSettings,
        supported_constraints: &MediaTrackSupportedConstraints,
    ) -> Self {
        let sanitized_constraints = BareOrMandatoryMediaTrackConstraints::from_iter(
            ideal_settings
                .into_iter()
                .map(|(property, setting)| (property, setting.into())),
        )
        .into_resolved()
        .into_sanitized(supported_constraints);

        Self {
            sanitized_constraints,
        }
    }
}

impl TieBreakingPolicy for SelectIdealSettingsPolicy {
    fn select_candidate<'b, I>(&self, candidates: I) -> &'b MediaTrackSettings
    where
        I: Iterator<Item = &'b MediaTrackSettings>,
    {
        candidates
            .min_by_key(|settings| {
                let fitness_distance = self
                    .sanitized_constraints
                    .fitness_distance(settings)
                    .expect("Fitness distance should be positive.");
                NotNan::new(fitness_distance).expect("Expected non-NaN fitness distance.")
            })
            .expect("The `candidates` iterator should have produced at least one item.")
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum SelectSettingsError {
    Overconstrained(OverconstrainedError),
}

impl From<OverconstrainedError> for SelectSettingsError {
    fn from(error: OverconstrainedError) -> Self {
        Self::Overconstrained(error)
    }
}

pub fn select_settings<'a, I, P>(
    possible_settings: I,
    constraints: &SanitizedMediaTrackConstraints,
    policy: &P,
) -> Result<&'a MediaTrackSettings, SelectSettingsError>
where
    I: IntoIterator<Item = &'a MediaTrackSettings>,
    P: TieBreakingPolicy,
{
    let possible_settings = possible_settings.into_iter();

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

    // We can expect "sanitized" constraints to not contain empty constraints:
    debug_assert!(constraints
        .mandatory
        .iter()
        .all(|(_, constraint)| !constraint.is_empty()));

    // Obtain candidates by filtering possible settings, dropping those with infinite fitness distances:
    //
    // This function call corresponds to steps 3 & 4 of the `SelectSettings` algorithm:
    // https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings

    let mut candidates = select_feasible_candidates(possible_settings, &constraints.mandatory)?;

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
    sieve_by_advanced_constraints(&mut candidates, &constraints.advanced);

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

#[derive(Default)]
struct ConstraintFailureInfo {
    failures: usize,
    errors: HashSet<SettingFitnessDistanceError>,
}

fn select_feasible_candidates<'a, I>(
    possible_settings: I,
    basic_or_required_constraints: &SanitizedMediaTrackConstraintSet,
) -> Result<Vec<(&'a MediaTrackSettings, f64)>, OverconstrainedError>
where
    I: Iterator<Item = &'a MediaTrackSettings>,
{
    // As specified in step 3 of the `SelectSettings` algorithm:
    // https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings
    //
    // > For every possible settings dictionary of copy compute its fitness distance,
    // > treating bare values of properties as ideal values. Let candidates be the
    // > set of settings dictionaries for which the fitness distance is finite.

    let mut settings_len = 0;
    let mut candidates: Vec<(&'a MediaTrackSettings, f64)> = vec![];
    let mut failed_constraints: HashMap<String, ConstraintFailureInfo> = Default::default();

    for settings in possible_settings {
        settings_len += 1;
        match basic_or_required_constraints.fitness_distance(settings) {
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

/// Implements step 5 of the `SelectSettings` algorithm:
/// https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings
///
/// # Note:
/// This may change the order of items in `feasible_candidates`.
/// In practice however this is not a problem as we have to sort
/// it by fitness-distance eventually anyway.
fn sieve_by_advanced_constraints<'a>(
    feasible_candidates: &mut Vec<(&'a MediaTrackSettings, f64)>,
    advanced_constraints: &SanitizedAdvancedMediaTrackConstraints,
) {
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

    for advanced_constraint_set in advanced_constraints.iter() {
        let results: Vec<bool> = feasible_candidates
            .iter()
            .map(
                |(candidate, _)| match advanced_constraint_set.fitness_distance(candidate) {
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

        for (index, is_match) in results.iter().enumerate() {
            if !is_match {
                feasible_candidates.swap_remove(index);
            }
        }
    }
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
    struct Violation {
        constraint: String,
        settings: Vec<String>,
    }
    let mut violators_by_kind: HashMap<SettingFitnessDistanceErrorKind, Violation> =
        HashMap::default();

    for error in errors {
        let violation = violators_by_kind.entry(error.kind).or_insert(Violation {
            constraint: error.constraint.clone(),
            settings: vec![],
        });
        assert_eq!(violation.constraint, error.constraint);
        if let Some(setting) = error.setting {
            violation.settings.push(setting.clone());
        }
    }

    let formatted_reasons: Vec<_> = violators_by_kind
        .into_iter()
        .map(|(kind, violation)| {
            let kind_str = match kind {
                SettingFitnessDistanceErrorKind::Missing => "missing",
                SettingFitnessDistanceErrorKind::Mismatch => "a mismatch",
                SettingFitnessDistanceErrorKind::TooSmall => "too small",
                SettingFitnessDistanceErrorKind::TooLarge => "too large",
            };

            let mut settings = violation.settings;

            if settings.is_empty() {
                return format!("{} (does not satisfy {})", kind_str, violation.constraint);
            }

            settings.sort();

            format!(
                "{} ([{}] do not satisfy {})",
                kind_str,
                settings.join(", "),
                violation.constraint
            )
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
    let message = Some(format!("Setting was {}.", formatted_reason));
    OverconstrainedError {
        constraint,
        message,
    }
}
