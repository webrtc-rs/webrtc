use ordered_float::NotNan;

use crate::{
    algorithms::FitnessDistance, BareOrMandatoryMediaTrackConstraints, MediaTrackSettings,
    MediaTrackSupportedConstraints, SanitizedMandatoryMediaTrackConstraints,
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
        I: IntoIterator<Item = &'a MediaTrackSettings>;
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
