use crate::ResolvedValueSequenceConstraint;

use super::{
    setting::SettingFitnessDistanceError, FitnessDistance, SettingFitnessDistanceErrorKind,
};

macro_rules! impl_non_numeric_value_sequence_constraint {
    (setting: $s:ty, constraint: $c:ty) => {
        impl<'a> FitnessDistance<Option<&'a $s>> for ResolvedValueSequenceConstraint<$c>
        where
            $s: PartialEq<$c>,
        {
            type Error = SettingFitnessDistanceError;

            fn fitness_distance(&self, setting: Option<&'a $s>) -> Result<f64, Self::Error> {
                if let Some(exact) = self.exact.as_ref() {
                    // As specified in step 2 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > If the constraint is required (constraintValue either contains
                    // > one or more members named […] 'exact' […]), and the settings
                    // > dictionary's constraintName member's value does not satisfy the
                    // > constraint or doesn't exist, the fitness distance is positive infinity.
                    match setting {
                        Some(actual) if exact.contains(actual) => {}
                        Some(setting) => {
                            return Err(SettingFitnessDistanceError {
                                kind: SettingFitnessDistanceErrorKind::Mismatch,
                                constraint: format!("{}", self.to_required_only()),
                                setting: Some(format!("{:?}", setting)),
                            })
                        }
                        None => {
                            return Err(SettingFitnessDistanceError {
                                kind: SettingFitnessDistanceErrorKind::Missing,
                                constraint: format!("{}", self.to_required_only()),
                                setting: None,
                            })
                        }
                    };
                }

                if let Some(ideal) = self.ideal.as_ref() {
                    // As specified in step 8 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > For all string, enum and boolean constraints […],
                    // > the fitness distance is the result of the formula:
                    // >
                    // > ```
                    // > (actual == ideal) ? 0 : 1
                    // > ```
                    //
                    // As well as step 5 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > If the settings dictionary's `constraintName` member
                    // > does not exist, the fitness distance is 1.
                    match setting {
                        Some(actual) if ideal.contains(actual) => Ok(0.0),
                        Some(_) => Ok(1.0),
                        None => Ok(1.0),
                    }
                } else {
                    // As specified in step 6 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > If no ideal value is specified (constraintValue either
                    // > contains no member named 'ideal', or, if bare values are to be
                    // > treated as 'ideal', isn't a bare value), the fitness distance is 0.
                    Ok(0.0)
                }
            }
        }
    };
}

impl_non_numeric_value_sequence_constraint!(setting: bool, constraint: bool);
impl_non_numeric_value_sequence_constraint!(setting: String, constraint: String);

macro_rules! impl_numeric_value_sequence_constraint {
    (setting: $s:ty, constraint: $c:ty) => {
        impl<'a> FitnessDistance<Option<&'a $s>> for ResolvedValueSequenceConstraint<$c> {
            type Error = SettingFitnessDistanceError;

            fn fitness_distance(&self, setting: Option<&'a $s>) -> Result<f64, Self::Error> {
                if let Some(exact) = &self.exact {
                    // As specified in step 2 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > If the constraint is required (constraintValue either contains
                    // > one or more members named […] 'exact' […]), and the settings
                    // > dictionary's constraintName member's value does not satisfy the
                    // > constraint or doesn't exist, the fitness distance is positive infinity.
                    match setting {
                        Some(&actual) if exact.contains(&(actual as $c)) => {}
                        Some(setting) => {
                            return Err(SettingFitnessDistanceError {
                                kind: SettingFitnessDistanceErrorKind::Mismatch,
                                constraint: format!("{}", self.to_required_only()),
                                setting: Some(format!("{:?}", setting)),
                            })
                        }
                        None => {
                            return Err(SettingFitnessDistanceError {
                                kind: SettingFitnessDistanceErrorKind::Missing,
                                constraint: format!("{}", self.to_required_only()),
                                setting: None,
                            })
                        }
                    };
                }

                if let Some(ideal) = &self.ideal {
                    // As specified in step 8 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > For all string, enum and boolean constraints […],
                    // > the fitness distance is the result of the formula:
                    // >
                    // > ```
                    // > (actual == ideal) ? 0 : 1
                    // > ```
                    //
                    // As well as step 5 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > If the settings dictionary's `constraintName` member
                    // > does not exist, the fitness distance is 1.
                    match setting {
                        Some(&actual) => {
                            let actual: f64 = actual as f64;
                            let mut min_fitness_distance = 1.0;
                            for ideal in ideal.into_iter() {
                                let ideal: f64 = (*ideal) as f64;
                                // As specified in step 7 of the `fitness distance` algorithm:
                                // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                                //
                                // > For all positive numeric constraints […],
                                // > the fitness distance is the result of the formula
                                // >
                                // > ```
                                // > (actual == ideal) ? 0 : |actual - ideal| / max(|actual|, |ideal|)
                                // > ```
                                let fitness_distance =
                                    super::relative_fitness_distance(actual, ideal);
                                if fitness_distance < min_fitness_distance {
                                    min_fitness_distance = fitness_distance;
                                }
                            }
                            Ok(min_fitness_distance)
                        }
                        None => Ok(1.0),
                    }
                } else {
                    // As specified in step 6 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > If no ideal value is specified (constraintValue either
                    // > contains no member named 'ideal', or, if bare values are to be
                    // > treated as 'ideal', isn't a bare value), the fitness distance is 0.
                    Ok(0.0)
                }
            }
        }
    };
}

impl_numeric_value_sequence_constraint!(setting: f64, constraint: f64);
impl_numeric_value_sequence_constraint!(setting: i64, constraint: u64);
impl_numeric_value_sequence_constraint!(setting: i64, constraint: f64);
impl_numeric_value_sequence_constraint!(setting: f64, constraint: u64);

#[cfg(test)]
mod tests;
