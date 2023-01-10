use crate::constraint::EmptyConstraint;

use super::{setting::SettingFitnessDistanceError, FitnessDistance};

impl<'a, T> FitnessDistance<Option<&'a T>> for EmptyConstraint {
    type Error = SettingFitnessDistanceError;

    fn fitness_distance(&self, _setting: Option<&'a T>) -> Result<f64, Self::Error> {
        // As specified in step 1 of the `SelectSettings` algorithm:
        // <https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings>
        //
        // > If an empty list has been given as the value for a constraint,
        // > it MUST be interpreted as if the constraint were not specified
        // > (in other words, an empty constraint == no constraint).
        Ok(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Constraint = EmptyConstraint;

    macro_rules! test_empty_constraint {
        (
            settings: $t:ty => $s:expr,
            expected: $e:expr $(,)?
        ) => {
            let settings: &[Option<$t>] = $s;
            let constraint = &Constraint {};
            for setting in settings {
                let actual = constraint.fitness_distance(setting.as_ref());

                assert_eq!(actual, $e);
            }
        };
    }

    #[test]
    fn bool() {
        test_empty_constraint!(
            settings: bool => &[None, Some(false)],
            expected: Ok(0.0)
        );
    }

    #[test]
    fn string() {
        test_empty_constraint!(
            settings: String => &[None, Some("foo".to_owned())],
            expected: Ok(0.0)
        );
    }

    #[test]
    fn i64() {
        test_empty_constraint!(
            settings: i64 => &[None, Some(42)],
            expected: Ok(0.0)
        );
    }

    #[test]
    fn f64() {
        test_empty_constraint!(
            settings: f64 => &[None, Some(42.0)],
            expected: Ok(0.0)
        );
    }
}
