use std::collections::HashMap;

use crate::{MediaTrackProperty, MediaTrackSettings, SanitizedMediaTrackConstraintSet};

use super::{setting::SettingFitnessDistanceError, FitnessDistance};

/// A list of media track properties and their corresponding fitness distance errors.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SettingsFitnessDistanceError {
    /// Setting errors per media track property.
    pub setting_errors: HashMap<MediaTrackProperty, SettingFitnessDistanceError>,
}

impl<'a> FitnessDistance<&'a MediaTrackSettings> for SanitizedMediaTrackConstraintSet {
    type Error = SettingsFitnessDistanceError;

    fn fitness_distance(&self, settings: &'a MediaTrackSettings) -> Result<f64, Self::Error> {
        let results: HashMap<MediaTrackProperty, _> = self
            .iter()
            .map(|(property, constraint)| {
                let setting = settings.get(property);
                let result = constraint.fitness_distance(setting);
                (property.clone(), result)
            })
            .collect();

        let mut total_fitness_distance = 0.0;

        let mut setting_errors: HashMap<MediaTrackProperty, SettingFitnessDistanceError> =
            Default::default();

        for (property, result) in results.into_iter() {
            match result {
                Ok(fitness_distance) => total_fitness_distance += fitness_distance,
                Err(error) => {
                    setting_errors.insert(property, error);
                }
            }
        }

        if setting_errors.is_empty() {
            Ok(total_fitness_distance)
        } else {
            Err(SettingsFitnessDistanceError { setting_errors })
        }
    }
}
