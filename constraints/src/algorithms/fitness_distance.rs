/// The function used to compute the "fitness distance" of a [setting][media_track_settings] value of a [`MediaStreamTrack`][media_stream_track] object.
///
/// # W3C Spec Compliance
///
/// The trait corresponds to the ["fitness distance"][fitness_distance] function in the W3C ["Media Capture and Streams"][media_capture_and_streams_spec] spec.
///
/// [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#dom-mediastreamtrack
/// [media_track_settings]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatracksettings
/// [fitness_distance]: https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance
/// [media_capture_and_streams_spec]: https://www.w3.org/TR/mediacapture-streams
pub trait FitnessDistance<Subject> {
    /// The type returned in the event of a computation error.
    type Error;

    /// Computes the fitness distance of the given `subject` in the range of `0.0..=1.0`.
    ///
    /// A distance of `0.0` denotes it maximally fit, one of `1.0` as maximally unfit.
    fn fitness_distance(&self, subject: Subject) -> Result<f64, Self::Error>;
}

mod empty_constraint;
mod setting;
mod settings;
mod value_constraint;
mod value_range_constraint;
mod value_sequence_constraint;

use std::cmp::Ordering;

pub use self::{
    setting::{SettingFitnessDistanceError, SettingFitnessDistanceErrorKind},
    settings::SettingsFitnessDistanceError,
};

fn nearly_cmp(lhs: f64, rhs: f64) -> Ordering {
    // Based on: https://stackoverflow.com/a/32334103/227536

    let epsilon: f64 = 128.0 * f64::EPSILON;
    let abs_th: f64 = f64::MIN;

    debug_assert!(epsilon < 1.0);

    if lhs == rhs {
        return Ordering::Equal;
    }

    let diff = (lhs - rhs).abs();
    let norm = (lhs.abs() + rhs.abs()).min(f64::MAX);

    if diff < (epsilon * norm).max(abs_th) {
        Ordering::Equal
    } else if lhs < rhs {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

fn is_nearly_greater_than_or_equal_to(actual: f64, min: f64) -> bool {
    nearly_cmp(actual, min) != Ordering::Less
}

fn is_nearly_less_than_or_equal_to(actual: f64, max: f64) -> bool {
    nearly_cmp(actual, max) != Ordering::Greater
}

fn is_nearly_equal_to(actual: f64, exact: f64) -> bool {
    nearly_cmp(actual, exact) == Ordering::Equal
}

fn relative_fitness_distance(actual: f64, ideal: f64) -> f64 {
    // As specified in step 7 of the `fitness distance` algorithm:
    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
    //
    // > For all positive numeric constraints […],
    // > the fitness distance is the result of the formula
    // >
    // > ```
    // > (actual == ideal) ? 0 : |actual - ideal| / max(|actual|, |ideal|)
    // > ```
    if (actual - ideal).abs() < f64::EPSILON {
        0.0
    } else {
        let numerator = (actual - ideal).abs();
        let denominator = actual.abs().max(ideal.abs());
        if denominator.abs() < f64::EPSILON {
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

    mod relative_fitness_distance {
        #[test]
        fn zero_distance() {
            // Make sure we're not dividing by zero:
            assert_eq!(super::relative_fitness_distance(0.0, 0.0), 0.0);

            assert_eq!(super::relative_fitness_distance(0.5, 0.5), 0.0);
            assert_eq!(super::relative_fitness_distance(1.0, 1.0), 0.0);
            assert_eq!(super::relative_fitness_distance(2.0, 2.0), 0.0);
        }

        #[test]
        fn fract_distance() {
            assert_eq!(super::relative_fitness_distance(1.0, 2.0), 0.5);
            assert_eq!(super::relative_fitness_distance(2.0, 1.0), 0.5);

            assert_eq!(super::relative_fitness_distance(0.5, 1.0), 0.5);
            assert_eq!(super::relative_fitness_distance(1.0, 0.5), 0.5);

            assert_eq!(super::relative_fitness_distance(0.25, 0.5), 0.5);
            assert_eq!(super::relative_fitness_distance(0.5, 0.25), 0.5);
        }

        #[test]
        fn one_distance() {
            assert_eq!(super::relative_fitness_distance(0.0, 0.5), 1.0);
            assert_eq!(super::relative_fitness_distance(0.5, 0.0), 1.0);

            assert_eq!(super::relative_fitness_distance(0.0, 1.0), 1.0);
            assert_eq!(super::relative_fitness_distance(1.0, 0.0), 1.0);

            assert_eq!(super::relative_fitness_distance(0.0, 2.0), 1.0);
            assert_eq!(super::relative_fitness_distance(2.0, 0.0), 1.0);
        }
    }
}
