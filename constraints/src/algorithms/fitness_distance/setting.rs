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

        let result = match (setting, self) {
            // Empty constraint:
            (_, ResolvedMediaTrackConstraint::Empty(_)) => Ok(0.0),

            // Boolean setting:
            (Setting::Bool(_setting), Constraint::IntegerRange(_constraint)) => Ok(0.0),
            (Setting::Bool(_setting), Constraint::FloatRange(_constraint)) => Ok(0.0),
            (Setting::Bool(setting), Constraint::Bool(constraint)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Setting::Bool(_setting), Constraint::StringSequence(_constraint)) => Ok(0.0),
            (Setting::Bool(_setting), Constraint::String(_constraint)) => Ok(0.0),

            // Integer setting:
            (Setting::Integer(setting), Constraint::IntegerRange(constraint)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Setting::Integer(setting), Constraint::FloatRange(constraint)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Setting::Integer(setting), Constraint::Bool(constraint)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Setting::Integer(_setting), Constraint::StringSequence(_constraint)) => Ok(0.0),
            (Setting::Integer(_setting), Constraint::String(_constraint)) => Ok(0.0),

            // Float setting:
            (Setting::Float(setting), Constraint::IntegerRange(constraint)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Setting::Float(setting), Constraint::FloatRange(constraint)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Setting::Float(setting), Constraint::Bool(constraint)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Setting::Float(_setting), Constraint::StringSequence(_constraint)) => Ok(0.0),
            (Setting::Float(_setting), Constraint::String(_constraint)) => Ok(0.0),

            // String setting:
            (Setting::String(_setting), Constraint::IntegerRange(_constraint)) => Ok(0.0),
            (Setting::String(_setting), Constraint::FloatRange(_constraint)) => Ok(0.0),
            (Setting::String(setting), Constraint::Bool(constraint)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Setting::String(setting), Constraint::StringSequence(constraint)) => {
                constraint.fitness_distance(Some(setting))
            }
            (Setting::String(setting), Constraint::String(constraint)) => {
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
