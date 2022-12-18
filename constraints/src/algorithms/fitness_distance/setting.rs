use crate::{MediaTrackSetting, ResolvedMediaTrackConstraint};

use super::FitnessDistance;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct SettingFitnessDistanceError {
    pub kind: SettingFitnessDistanceErrorKind,
    pub constraint: String,
    pub setting: Option<String>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SettingFitnessDistanceErrorKind {
    Missing,
    Mismatch,
    TooSmall,
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
