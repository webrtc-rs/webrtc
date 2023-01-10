use std::collections::HashSet;

use crate::{
    algorithms::fitness_distance::SettingFitnessDistanceError, errors::OverconstrainedError,
    MediaTrackSettings, SanitizedMediaTrackConstraints,
};

mod apply_advanced;
mod apply_mandatory;
mod select_optimal;
mod tie_breaking;

pub use self::tie_breaking::*;

use self::{apply_advanced::*, apply_mandatory::*, select_optimal::*};

/// A mode indicating whether device information may be exposed.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum DeviceInformationExposureMode {
    /// Device information may be exposed.
    Exposed,
    /// Device information may NOT be exposed.
    Protected,
}

/// An error type indicating a failure of the `SelectSettings` algorithm.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum SelectSettingsError {
    /// An error caused by one or more over-constrained settings.
    Overconstrained(OverconstrainedError),
}

impl From<OverconstrainedError> for SelectSettingsError {
    fn from(error: OverconstrainedError) -> Self {
        Self::Overconstrained(error)
    }
}

/// This function implements steps 1-5 of the `SelectSettings` algorithm
/// as defined by the W3C spec:
/// <https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings>
///
/// Step 6 (tie-breaking) is omitted by this implementation and expected to be performed
/// manually on the returned candidates.
/// For this several implementation of `TieBreakingPolicy` are provided by this crate.
pub fn select_settings_candidates<'a, I>(
    possible_settings: I,
    constraints: &SanitizedMediaTrackConstraints,
    exposure_mode: DeviceInformationExposureMode,
) -> Result<Vec<&'a MediaTrackSettings>, SelectSettingsError>
where
    I: IntoIterator<Item = &'a MediaTrackSettings>,
{
    let possible_settings = possible_settings.into_iter();

    // As specified in step 1 of the `SelectSettings` algorithm:
    // <https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings>
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

    // We expect "sanitized" constraints to not contain empty constraints:
    debug_assert!(constraints
        .mandatory
        .iter()
        .all(|(_, constraint)| !constraint.is_empty()));

    // Obtain candidates by filtering possible settings, dropping those with infinite fitness distances:
    //
    // This function call corresponds to steps 3 & 4 of the `SelectSettings` algorithm:
    // <https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings>

    let candidates_and_fitness_distances =
        apply_mandatory_constraints(possible_settings, &constraints.mandatory, exposure_mode)?;

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
    let candidates =
        apply_advanced_constraints(candidates_and_fitness_distances, &constraints.advanced);

    // As specified in step 6 of the `SelectSettings` algorithm:
    // <https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings>
    //
    // > Select one settings dictionary from candidates, and return it as the result of the `SelectSettings` algorithm.
    // > The User Agent MUST use one with the smallest fitness distance, as calculated in step 3.
    // > If more than one settings dictionary have the smallest fitness distance,
    // > the User Agent chooses one of them based on system default property values and User Agent default property values.
    //
    // # Important
    // Instead of return just ONE settings instance "with the smallest fitness distance, as calculated in step 3"
    // we instead return ALL settings instances "with the smallest fitness distance, as calculated in step 3"
    // and leave tie-breaking to the User Agent in a seperate step:
    Ok(select_optimal_candidates(candidates))
}

#[derive(Default)]
pub(crate) struct ConstraintFailureInfo {
    pub(crate) failures: usize,
    pub(crate) errors: HashSet<SettingFitnessDistanceError>,
}

#[cfg(test)]
mod tests;
