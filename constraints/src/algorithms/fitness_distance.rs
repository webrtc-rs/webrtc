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
