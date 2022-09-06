use crate::{MediaTrackConstraint, MediaTrackSetting};

use super::FitnessDistance;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum SettingFitnessDistanceError {
    Missing,
    Mismatch,
    TooSmall,
    TooLarge,
    Invalid,
}

impl<'a> FitnessDistance<Option<&'a MediaTrackSetting>> for MediaTrackConstraint {
    type Error = SettingFitnessDistanceError;

    fn fitness_distance(&self, setting: Option<&'a MediaTrackSetting>) -> Result<f64, Self::Error> {
        type Setting = MediaTrackSetting;
        type Constraint = MediaTrackConstraint;

        let setting = match setting {
            Some(setting) => setting,
            None => {
                return if self.is_required() {
                    Err(Self::Error::Missing)
                } else {
                    Ok(1.0)
                }
            }
        };

        let result = match (setting, self) {
            // Empty constraint:
            (_, MediaTrackConstraint::Empty(_)) => Ok(0.0),

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

        match result {
            Ok(fitness_distance) => {
                if fitness_distance.is_finite() {
                    Ok(fitness_distance)
                } else {
                    Err(Self::Error::Invalid)
                }
            }
            Err(error) => Err(error),
        }
    }
}
