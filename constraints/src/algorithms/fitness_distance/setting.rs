use crate::{MediaTrackSetting, ResolvedMediaTrackConstraint};

use super::FitnessDistance;

/// An error indicating a rejected fitness distance computation,
/// likely caused by a mismatched yet required constraint.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct SettingFitnessDistanceError {
    /// The kind of the error (e.g. missing value, mismatching value, …).
    pub kind: SettingFitnessDistanceErrorKind,
    /// The required constraint value.
    pub constraint: String,
    /// The offending setting value.
    pub setting: Option<String>,
}

/// The kind of the error (e.g. missing value, mismatching value, …).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SettingFitnessDistanceErrorKind {
    /// Settings value is missing.
    Missing,
    /// Settings value is a mismatch.
    Mismatch,
    /// Settings value is too small.
    TooSmall,
    /// Settings value is too large.
    TooLarge,
}

impl<'a> FitnessDistance<Option<&'a MediaTrackSetting>> for ResolvedMediaTrackConstraint {
    type Error = SettingFitnessDistanceError;

    fn fitness_distance(&self, setting: Option<&'a MediaTrackSetting>) -> Result<f64, Self::Error> {
        type Setting = MediaTrackSetting;
        type Constraint = ResolvedMediaTrackConstraint;

        let setting = match setting {
            Some(setting) => setting,
            None => {
                return if self.is_required() {
                    Err(Self::Error {
                        kind: SettingFitnessDistanceErrorKind::Missing,
                        constraint: format!("{}", self.to_required_only()),
                        setting: None,
                    })
                } else {
                    Ok(1.0)
                }
            }
        };

        let result = match (self, setting) {
            // Empty constraint:
            (ResolvedMediaTrackConstraint::Empty(constraint), setting) => {
                constraint.fitness_distance(Some(setting))
            }

            // Boolean constraint:
            (Constraint::Bool(constraint), Setting::Bool(setting)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Constraint::Bool(constraint), Setting::Integer(setting)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Constraint::Bool(constraint), Setting::Float(setting)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Constraint::Bool(constraint), Setting::String(setting)) => {
                constraint.fitness_distance(Some(setting))
            }

            // Integer constraint:
            (Constraint::IntegerRange(_constraint), Setting::Bool(_setting)) => Ok(0.0),
            (Constraint::IntegerRange(constraint), Setting::Integer(setting)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Constraint::IntegerRange(constraint), Setting::Float(setting)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Constraint::IntegerRange(_constraint), Setting::String(_setting)) => Ok(0.0),

            // Float constraint:
            (Constraint::FloatRange(_constraint), Setting::Bool(_setting)) => Ok(0.0),
            (Constraint::FloatRange(constraint), Setting::Integer(setting)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Constraint::FloatRange(constraint), Setting::Float(setting)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Constraint::FloatRange(_constraint), Setting::String(_setting)) => Ok(0.0),

            // String constraint:
            (Constraint::String(_constraint), Setting::Bool(_setting)) => Ok(0.0),
            (Constraint::String(_constraint), Setting::Integer(_setting)) => Ok(0.0),
            (Constraint::String(_constraint), Setting::Float(_setting)) => Ok(0.0),
            (Constraint::String(constraint), Setting::String(setting)) => {
                constraint.fitness_distance(Some(setting))
            }

            // String sequence constraint:
            (Constraint::StringSequence(_constraint), Setting::Bool(_setting)) => Ok(0.0),
            (Constraint::StringSequence(_constraint), Setting::Integer(_setting)) => Ok(0.0),
            (Constraint::StringSequence(_constraint), Setting::Float(_setting)) => Ok(0.0),
            (Constraint::StringSequence(constraint), Setting::String(setting)) => {
                constraint.fitness_distance(Some(setting))
            }
        };

        #[cfg(debug_assertions)]
        if let Ok(fitness_distance) = result {
            debug_assert!({ fitness_distance.is_finite() });
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use crate::{constraint::EmptyConstraint, MediaTrackSetting, ResolvedMediaTrackConstraint};

    use super::*;

    #[test]
    fn empty_constraint() {
        // As per step 1 of the `SelectSettings` algorithm from the W3C spec:
        // <https://www.w3.org/TR/mediacapture-streams/#dfn-selectsettings>
        //
        // > Each constraint specifies one or more values (or a range of values) for its property.
        // > A property MAY appear more than once in the list of 'advanced' ConstraintSets.
        // > If an empty list has been given as the value for a constraint,
        // > it MUST be interpreted as if the constraint were not specified
        // > (in other words, an empty constraint == no constraint).
        let constraint = ResolvedMediaTrackConstraint::Empty(EmptyConstraint {});

        let settings = [
            MediaTrackSetting::Bool(true),
            MediaTrackSetting::Integer(42),
            MediaTrackSetting::Float(4.2),
            MediaTrackSetting::String("string".to_owned()),
        ];

        let expected = 0.0;

        for setting in settings {
            let actual = constraint.fitness_distance(Some(&setting)).unwrap();

            assert_eq!(actual, expected);
        }
    }

    mod bool_constraint {
        use crate::ResolvedValueConstraint;

        use super::*;

        #[test]
        fn bool_setting() {
            // As per step 8 of the `fitness distance` function from the W3C spec:
            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
            //
            // > For all string, enum and boolean constraints
            // > (e.g. deviceId, groupId, facingMode, resizeMode, echoCancellation),
            // > the fitness distance is the result of the formula:
            // >
            // > ```
            // > (actual == ideal) ? 0 : 1
            // > ```

            let scenarios = [(false, false), (false, true), (true, false), (true, true)];

            for (constraint_value, setting_value) in scenarios {
                let constraint = ResolvedMediaTrackConstraint::Bool(ResolvedValueConstraint {
                    exact: None,
                    ideal: Some(constraint_value),
                });

                let setting = MediaTrackSetting::Bool(setting_value);

                let actual = constraint.fitness_distance(Some(&setting)).unwrap();

                let expected = if constraint_value == setting_value {
                    0.0
                } else {
                    1.0
                };

                assert_eq!(actual, expected);
            }
        }

        #[test]
        fn non_bool_settings() {
            // As per step 4 of the `fitness distance` function from the W3C spec:
            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
            //
            // > If constraintValue is a boolean, but the constrainable property is not,
            // > then the fitness distance is based on whether the settings dictionary's
            // > constraintName member exists or not, from the formula:
            // >
            // > ```
            // > (constraintValue == exists) ? 0 : 1
            // > ```

            let settings = [
                MediaTrackSetting::Integer(42),
                MediaTrackSetting::Float(4.2),
                MediaTrackSetting::String("string".to_owned()),
            ];

            let scenarios = [(false, false), (false, true), (true, false), (true, true)];

            for (constraint_value, setting_value) in scenarios {
                let constraint = ResolvedMediaTrackConstraint::Bool(ResolvedValueConstraint {
                    exact: None,
                    ideal: Some(constraint_value),
                });

                for setting in settings.iter() {
                    // TODO: Replace `if { Some(_) } else { None }` with `.then_some(_)`
                    // once MSRV has passed 1.62.0:
                    let setting = if setting_value { Some(setting) } else { None };
                    let actual = constraint.fitness_distance(setting).unwrap();

                    let expected = if setting_value { 0.0 } else { 1.0 };

                    assert_eq!(actual, expected);
                }
            }
        }
    }

    mod numeric_constraint {
        use crate::ResolvedValueRangeConstraint;

        use super::*;

        #[test]
        fn missing_settings() {
            // As per step 5 of the `fitness distance` function from the W3C spec:
            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
            //
            // > If the settings dictionary's constraintName member does not exist,
            // > the fitness distance is 1.

            let constraints = [
                ResolvedMediaTrackConstraint::IntegerRange(ResolvedValueRangeConstraint {
                    exact: None,
                    ideal: Some(42),
                    min: None,
                    max: None,
                }),
                ResolvedMediaTrackConstraint::FloatRange(ResolvedValueRangeConstraint {
                    exact: None,
                    ideal: Some(42.0),
                    min: None,
                    max: None,
                }),
            ];

            for constraint in constraints {
                let actual = constraint.fitness_distance(None).unwrap();

                let expected = 1.0;

                assert_eq!(actual, expected);
            }
        }

        #[test]
        fn compatible_settings() {
            // As per step 7 of the `fitness distance` function from the W3C spec:
            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
            //
            // > For all positive numeric constraints
            // > (such as height, width, frameRate, aspectRatio, sampleRate and sampleSize),
            // > the fitness distance is the result of the formula
            // >
            // > ```
            // > (actual == ideal) ? 0 : |actual - ideal| / max(|actual|, |ideal|)
            // > ```

            let settings = [
                MediaTrackSetting::Integer(21),
                MediaTrackSetting::Float(21.0),
            ];

            let constraints = [
                ResolvedMediaTrackConstraint::IntegerRange(ResolvedValueRangeConstraint {
                    exact: None,
                    ideal: Some(42),
                    min: None,
                    max: None,
                }),
                ResolvedMediaTrackConstraint::FloatRange(ResolvedValueRangeConstraint {
                    exact: None,
                    ideal: Some(42.0),
                    min: None,
                    max: None,
                }),
            ];

            for constraint in constraints {
                for setting in settings.iter() {
                    let actual = constraint.fitness_distance(Some(setting)).unwrap();

                    let expected = 0.5;

                    assert_eq!(actual, expected);
                }
            }
        }

        #[test]
        fn incompatible_settings() {
            // As per step 3 of the `fitness distance` function from the W3C spec:
            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
            //
            // > If the constraint does not apply for this type of object, the fitness distance is 0
            // > (that is, the constraint does not influence the fitness distance).

            let settings = [
                MediaTrackSetting::Bool(true),
                MediaTrackSetting::String("string".to_owned()),
            ];

            let constraints = [
                ResolvedMediaTrackConstraint::IntegerRange(ResolvedValueRangeConstraint {
                    exact: None,
                    ideal: Some(42),
                    min: None,
                    max: None,
                }),
                ResolvedMediaTrackConstraint::FloatRange(ResolvedValueRangeConstraint {
                    exact: None,
                    ideal: Some(42.0),
                    min: None,
                    max: None,
                }),
            ];

            for constraint in constraints {
                for setting in settings.iter() {
                    let actual = constraint.fitness_distance(Some(setting)).unwrap();

                    let expected = 0.0;

                    println!("constraint: {constraint:?}");
                    println!("setting: {setting:?}");
                    println!("actual: {actual:?}");
                    println!("expected: {expected:?}");

                    assert_eq!(actual, expected);
                }
            }
        }
    }

    mod string_constraint {
        use crate::ResolvedValueConstraint;

        use super::*;

        #[test]
        fn missing_settings() {
            // As per step 5 of the `fitness distance` function from the W3C spec:
            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
            //
            // > If the settings dictionary's constraintName member does not exist,
            // > the fitness distance is 1.

            let constraint = ResolvedMediaTrackConstraint::String(ResolvedValueConstraint {
                exact: None,
                ideal: Some("constraint".to_owned()),
            });

            let actual = constraint.fitness_distance(None).unwrap();

            let expected = 1.0;

            assert_eq!(actual, expected);
        }

        #[test]
        fn compatible_settings() {
            // As per step 8 of the `fitness distance` function from the W3C spec:
            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
            //
            // > For all string, enum and boolean constraints
            // > (e.g. deviceId, groupId, facingMode, resizeMode, echoCancellation),
            // > the fitness distance is the result of the formula:
            // >
            // > ```
            // > (actual == ideal) ? 0 : 1
            // > ```

            let constraint = ResolvedMediaTrackConstraint::String(ResolvedValueConstraint {
                exact: None,
                ideal: Some("constraint".to_owned()),
            });

            let settings = [MediaTrackSetting::String("setting".to_owned())];

            for setting in settings {
                let actual = constraint.fitness_distance(Some(&setting)).unwrap();

                let expected = 1.0;

                assert_eq!(actual, expected);
            }
        }

        #[test]
        fn incompatible_settings() {
            // As per step 3 of the `fitness distance` function from the W3C spec:
            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
            //
            // > If the constraint does not apply for this type of object, the fitness distance is 0
            // > (that is, the constraint does not influence the fitness distance).

            let constraint = ResolvedMediaTrackConstraint::String(ResolvedValueConstraint {
                exact: None,
                ideal: Some("string".to_owned()),
            });

            let settings = [
                MediaTrackSetting::Bool(true),
                MediaTrackSetting::Integer(42),
                MediaTrackSetting::Float(4.2),
            ];

            for setting in settings {
                let actual = constraint.fitness_distance(Some(&setting)).unwrap();

                let expected = 0.0;

                println!("constraint: {constraint:?}");
                println!("setting: {setting:?}");
                println!("actual: {actual:?}");
                println!("expected: {expected:?}");

                assert_eq!(actual, expected);
            }
        }
    }

    mod string_sequence_constraint {
        use crate::ResolvedValueSequenceConstraint;

        use super::*;

        #[test]
        fn missing_settings() {
            // As per step 5 of the `fitness distance` function from the W3C spec:
            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
            //
            // > If the settings dictionary's constraintName member does not exist,
            // > the fitness distance is 1.

            let constraint =
                ResolvedMediaTrackConstraint::StringSequence(ResolvedValueSequenceConstraint {
                    exact: None,
                    ideal: Some(vec!["constraint".to_owned()]),
                });

            let actual = constraint.fitness_distance(None).unwrap();

            let expected = 1.0;

            assert_eq!(actual, expected);
        }

        #[test]
        fn compatible_settings() {
            // As per step 8 of the `fitness distance` function from the W3C spec:
            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
            //
            // > For all string, enum and boolean constraints
            // > (e.g. deviceId, groupId, facingMode, resizeMode, echoCancellation),
            // > the fitness distance is the result of the formula:
            // >
            // > ```
            // > (actual == ideal) ? 0 : 1
            // > ```
            //
            // As well as the preliminary definition:
            //
            // > For string valued constraints, we define "==" below to be true if one of the
            // > values in the sequence is exactly the same as the value being compared against.

            let constraint =
                ResolvedMediaTrackConstraint::StringSequence(ResolvedValueSequenceConstraint {
                    exact: None,
                    ideal: Some(vec!["constraint".to_owned()]),
                });

            let settings = [MediaTrackSetting::String("setting".to_owned())];

            for setting in settings {
                let actual = constraint.fitness_distance(Some(&setting)).unwrap();

                let expected = 1.0;

                assert_eq!(actual, expected);
            }
        }

        #[test]
        fn incompatible_settings() {
            // As per step 3 of the `fitness distance` function from the W3C spec:
            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
            //
            // > If the constraint does not apply for this type of object, the fitness distance is 0
            // > (that is, the constraint does not influence the fitness distance).

            let constraint =
                ResolvedMediaTrackConstraint::StringSequence(ResolvedValueSequenceConstraint {
                    exact: None,
                    ideal: Some(vec!["constraint".to_owned()]),
                });

            let settings = [
                MediaTrackSetting::Bool(true),
                MediaTrackSetting::Integer(42),
                MediaTrackSetting::Float(4.2),
            ];

            for setting in settings {
                let actual = constraint.fitness_distance(Some(&setting)).unwrap();

                let expected = 0.0;

                assert_eq!(actual, expected);
            }
        }
    }
}
