use crate::constraint::ResolvedValueConstraint;

use super::{
    setting::SettingFitnessDistanceError, FitnessDistance, SettingFitnessDistanceErrorKind,
};

// Standard implementation for value constraints of arbitrary `Setting` and `Constraint`
// types where `Setting: PartialEq<Constraint>`:
macro_rules! impl_non_numeric_value_constraint {
    (setting: $s:ty, constraint: $c:ty) => {
        impl<'a> FitnessDistance<Option<&'a $s>> for ResolvedValueConstraint<$c>
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
                        Some(actual) if actual == exact => {}
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
                    match setting {
                        Some(actual) if actual == ideal => {
                            // As specified in step 8 of the `fitness distance` algorithm:
                            // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                            //
                            // > For all string, enum and boolean constraints […],
                            // > the fitness distance is the result of the formula:
                            // >
                            // > ```
                            // > (actual == ideal) ? 0 : 1
                            // > ```
                            Ok(0.0)
                        }
                        _ => {
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

impl_non_numeric_value_constraint!(setting: bool, constraint: bool);
impl_non_numeric_value_constraint!(setting: String, constraint: String);

// Specialized implementations for floating-point value constraints (and settings):

macro_rules! impl_numeric_value_constraint {
    (setting: $s:ty, constraint: $c:ty) => {
        impl<'a> FitnessDistance<Option<&'a $s>> for ResolvedValueConstraint<$c> {
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

impl_numeric_value_constraint!(setting: f64, constraint: f64);
impl_numeric_value_constraint!(setting: i64, constraint: u64);
impl_numeric_value_constraint!(setting: i64, constraint: f64);
impl_numeric_value_constraint!(setting: f64, constraint: u64);

// Specialized implementations for boolean value constraints of mis-matching
// and thus either "existence"-checked or ignored setting types:
macro_rules! impl_exists_value_constraint {
    (settings: [$($s:ty),+], constraint: bool) => {
        $(impl_exists_value_constraint!(setting: $s, constraint: bool);)+
    };
    (setting: $s:ty, constraint: bool) => {
        impl<'a> FitnessDistance<Option<&'a $s>> for ResolvedValueConstraint<bool> {
            type Error = SettingFitnessDistanceError;

            fn fitness_distance(&self, setting: Option<&'a $s>) -> Result<f64, Self::Error> {
                // A bare boolean value (as described in step 4 of the
                // `fitness distance` algorithm) gets parsed as:
                // ```
                // ResolvedValueConstraint::<bool> {
                //     exact: Some(bare),
                //     ideal: None,
                // }
                // ```
                //
                // For all other configurations we just interpret it as an incompatible constraint.
                match self.exact {
                    // As specified in step 4 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > If constraintValue is a boolean, but the constrainable property is not,
                    // > then the fitness distance is based on whether the settings dictionary's
                    // > `constraintName` member exists or not, from the formula:
                    // >
                    // > ```
                    // > (constraintValue == exists) ? 0 : 1
                    // > ```
                    Some(expected) => {
                        if setting.is_some() == expected {
                            Ok(0.0)
                        } else {
                            Ok(1.0)
                        }
                    }
                    // As specified in step 3 of the `fitness distance` algorithm:
                    // <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>
                    //
                    // > If the constraint does not apply for this type of object,
                    // > the fitness distance is 0 (that is, the constraint does not
                    // > influence the fitness distance).
                    None => Ok(0.0),
                }
            }
        }
    };
}

impl_exists_value_constraint!(settings: [String, i64, f64], constraint: bool);

#[cfg(test)]
mod tests;
