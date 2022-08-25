pub trait FitnessDistance<Subject> {
    type Error;

    fn fitness_distance(&self, subject: Subject) -> Result<f64, Self::Error>;
}

mod empty_constraint;
mod setting;
mod settings;
mod value_constraint;
mod value_range_constraint;
mod value_sequence_constraint;

pub use self::{
    setting::{SettingFitnessDistanceError, SettingFitnessDistanceErrorKind},
    settings::SettingsFitnessDistanceError,
};

fn relative_fitness_distance(actual: f64, ideal: f64) -> f64 {
    let actual: f64 = actual as f64;
    let ideal: f64 = ideal as f64;

    // As specified in step 7 of the `fitness distance` algorithm:
    // https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance
    //
    // > For all positive numeric constraints […],
    // > the fitness distance is the result of the formula
    // >
    // > ```
    // > (actual == ideal) ? 0 : |actual - ideal| / max(|actual|, |ideal|)
    // > ```
    if actual == ideal {
        0.0
    } else {
        let numerator = (actual - ideal).abs();
        let denominator = actual.abs().max(ideal.abs());
        if denominator == 0.0 {
            // Avoid division by zero crashes:
            0.0
        } else {
            numerator / denominator
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_fitness_distance() {
        assert_eq!(super::relative_fitness_distance(0.0, 1.0), 1.0);
        assert_eq!(super::relative_fitness_distance(1.0, 0.0), 1.0);

        assert_eq!(super::relative_fitness_distance(0.0, 0.1), 1.0);
        assert_eq!(super::relative_fitness_distance(0.1, 0.0), 1.0);

        // Make sure we're not dividing by zero:
        assert_eq!(super::relative_fitness_distance(0.0, 0.0), 0.0);
    }
}
