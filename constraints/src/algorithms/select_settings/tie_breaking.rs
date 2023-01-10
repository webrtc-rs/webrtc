use std::iter::FromIterator;

use ordered_float::NotNan;

use crate::{
    algorithms::FitnessDistance, MandatoryMediaTrackConstraints, MediaTrackSettings,
    MediaTrackSupportedConstraints, SanitizedMandatoryMediaTrackConstraints,
};

/// A tie-breaking policy used for selecting a single preferred candidate
/// from a set list of equally optimal setting candidates.
pub trait TieBreakingPolicy {
    /// Selects a preferred candidate from a non-empty selection of optimal candidates.
    ///
    /// As specified in step 6 of the `SelectSettings` algorithm:
    /// <https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings>
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
    /// Creates a new policy.
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
    /// Creates a new policy from the given ideal settings and supported constraints.
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::iter::FromIterator;

    use crate::{
        property::all::name::*, MediaTrackSettings, MediaTrackSupportedConstraints, ResizeMode,
    };

    #[test]
    fn first() {
        let settings = vec![
            MediaTrackSettings::from_iter([(&DEVICE_ID, "device-id-0".into())]),
            MediaTrackSettings::from_iter([(&DEVICE_ID, "device-id-1".into())]),
            MediaTrackSettings::from_iter([(&DEVICE_ID, "device-id-2".into())]),
        ];

        let policy = FirstPolicy::default();

        let actual = policy.select_candidate(&settings);

        let expected = &settings[0];

        assert_eq!(actual, expected);
    }

    #[test]
    fn closest_to_ideal() {
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

        let ideal_settings = vec![
            MediaTrackSettings::from_iter([(&HEIGHT, 450.into()), (&WIDTH, 700.into())]),
            MediaTrackSettings::from_iter([(&HEIGHT, 700.into()), (&WIDTH, 1250.into())]),
            MediaTrackSettings::from_iter([(&HEIGHT, 1000.into()), (&WIDTH, 2000.into())]),
            MediaTrackSettings::from_iter([(&HEIGHT, 1500.into()), (&WIDTH, 2500.into())]),
            MediaTrackSettings::from_iter([(&HEIGHT, 2000.into()), (&WIDTH, 3750.into())]),
        ];

        for (index, ideal) in ideal_settings.iter().enumerate() {
            let policy = ClosestToIdealPolicy::new(ideal.clone(), &supported_constraints);

            let actual = policy.select_candidate(&settings);

            let expected = &settings[index];

            assert_eq!(actual, expected);
        }
    }
}
