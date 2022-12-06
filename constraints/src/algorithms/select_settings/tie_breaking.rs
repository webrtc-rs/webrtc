use std::iter::FromIterator;

use ordered_float::NotNan;

use crate::{
    algorithms::FitnessDistance, MandatoryMediaTrackConstraints, MediaTrackSettings,
    MediaTrackSupportedConstraints, SanitizedMandatoryMediaTrackConstraints,
};

pub trait TieBreakingPolicy {
    /// Selects a preferred candidate from a non-empty selection of optimal candidates.
    ///
    /// As specified in step 6 of the `SelectSettings` algorithm:
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
        I: IntoIterator<Item = &'a MediaTrackSettings>;
}

/// A naïve tie-breaking policy that just picks the first settings item it encounters.
pub struct FirstPolicy;

impl FirstPolicy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FirstPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl TieBreakingPolicy for FirstPolicy {
    fn select_candidate<'a, I>(&self, candidates: I) -> &'a MediaTrackSettings
    where
        I: IntoIterator<Item = &'a MediaTrackSettings>,
    {
        // Safety: We know that `candidates is non-empty:
        candidates
            .into_iter()
            .next()
            .expect("The `candidates` iterator should have produced at least one item.")
    }
}

/// A tie-breaking policy that picks the settings item that's closest to the specified ideal settings.
pub struct ClosestToIdealPolicy {
    sanitized_constraints: SanitizedMandatoryMediaTrackConstraints,
}

impl ClosestToIdealPolicy {
    pub fn new(
        ideal_settings: MediaTrackSettings,
        supported_constraints: &MediaTrackSupportedConstraints,
    ) -> Self {
        let sanitized_constraints = MandatoryMediaTrackConstraints::from_iter(
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

impl TieBreakingPolicy for ClosestToIdealPolicy {
    fn select_candidate<'b, I>(&self, candidates: I) -> &'b MediaTrackSettings
    where
        I: IntoIterator<Item = &'b MediaTrackSettings>,
    {
        candidates
            .into_iter()
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
