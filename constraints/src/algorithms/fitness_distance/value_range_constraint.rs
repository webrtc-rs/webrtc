use crate::ResolvedValueRangeConstraint;

use super::{
    setting::SettingFitnessDistanceError, FitnessDistance, SettingFitnessDistanceErrorKind,
};

macro_rules! impl_value_range_constraint {
    (setting: $s:ty, constraint: $c:ty) => {
        impl<'a> FitnessDistance<Option<&'a $s>> for ResolvedValueRangeConstraint<$c> {
            type Error = SettingFitnessDistanceError;

            fn fitness_distance(&self, setting: Option<&'a $s>) -> Result<f64, Self::Error> {
                if let Some(exact) = self.exact {
                    // As specified in step 2 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > If the constraint is required (constraintValue either contains
                    // > one or more members named […] 'exact' […]), and the settings
                    // > dictionary's constraintName member's value does not satisfy the
                    // > constraint or doesn't exist, the fitness distance is positive infinity.
                    match setting {
                        Some(&actual) if super::is_nearly_equal_to(actual as f64, exact as f64) => {
                        }
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

                if let Some(min) = self.min {
                    // As specified in step 2 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > If the constraint is required (constraintValue either contains
                    // > one or more members named […] 'min' […]), and the settings
                    // > dictionary's constraintName member's value does not satisfy the
                    // > constraint or doesn't exist, the fitness distance is positive infinity.
                    match setting {
                        Some(&actual)
                            if super::is_nearly_greater_than_or_equal_to(
                                actual as f64,
                                min as f64,
                            ) => {}
                        Some(setting) => {
                            return Err(SettingFitnessDistanceError {
                                kind: SettingFitnessDistanceErrorKind::TooSmall,
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

                if let Some(max) = self.max {
                    // As specified in step 2 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > If the constraint is required (constraintValue either contains
                    // > one or more members named […] 'max' […]), and the settings
                    // > dictionary's constraintName member's value does not satisfy the
                    // > constraint or doesn't exist, the fitness distance is positive infinity.
                    match setting {
                        Some(&actual)
                            if super::is_nearly_less_than_or_equal_to(
                                actual as f64,
                                max as f64,
                            ) => {}
                        Some(setting) => {
                            return Err(SettingFitnessDistanceError {
                                kind: SettingFitnessDistanceErrorKind::TooLarge,
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

                if let Some(ideal) = self.ideal {
                    match setting {
                        Some(&actual) => {
                            let actual: f64 = actual as f64;
                            let ideal: f64 = ideal as f64;
                            // As specified in step 7 of the `fitness distance` algorithm:
                            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                            //
                            // > For all positive numeric constraints […],
                            // > the fitness distance is the result of the formula
                            // >
                            // > ```
                            // > (actual == ideal) ? 0 : |actual - ideal| / max(|actual|, |ideal|)
                            // > ```
                            Ok(super::relative_fitness_distance(actual, ideal))
                        }
                        None => {
                            // As specified in step 5 of the `fitness distance` algorithm:
                            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                            //
                            // > If the settings dictionary's `constraintName` member
                            // > does not exist, the fitness distance is 1.
                            Ok(1.0)
                        }
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

impl_value_range_constraint!(setting: f64, constraint: f64);
impl_value_range_constraint!(setting: i64, constraint: u64);
impl_value_range_constraint!(setting: i64, constraint: f64);
impl_value_range_constraint!(setting: f64, constraint: u64);

// Specialized implementations for non-boolean value constraints of mis-matching,
// and thus ignored setting types:
macro_rules! impl_ignored_value_range_constraint {
    (settings: [$($s:ty),+], constraint: $c:ty) => {
        $(impl_ignored_value_range_constraint!(setting: $s, constraint: $c);)+
    };
    (setting: $s:ty, constraint: $c:ty) => {
        impl<'a> FitnessDistance<Option<&'a $s>> for ResolvedValueRangeConstraint<$c> {
            type Error = SettingFitnessDistanceError;

            fn fitness_distance(&self, _setting: Option<&'a $s>) -> Result<f64, Self::Error> {
                // As specified in step 3 of the `fitness distance` algorithm:
                // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                //
                // > If the constraint does not apply for this type of object,
                // > the fitness distance is 0 (that is, the constraint does not
                // > influence the fitness distance).
                Ok(0.0)
            }
        }
    };
}

impl_ignored_value_range_constraint!(settings: [bool, String], constraint: u64);
impl_ignored_value_range_constraint!(settings: [bool, String], constraint: f64);

#[cfg(test)]
mod tests;
