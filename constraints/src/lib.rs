//! Pure Rust implementation of the constraint logic defined in the ["Media Capture and Streams"][mediacapture_streams] spec.
//!
//! [mediacapture_streams]: https://www.w3.org/TR/mediacapture-streams/

pub mod property;

mod macros;
mod setting;
mod settings;
mod supported_constraints;

#[allow(unused_imports)]
pub use self::{
    setting::MediaTrackSetting, settings::MediaTrackSettings,
    supported_constraints::MediaTrackSupportedConstraints,
};

#[allow(unused_imports)]
pub(crate) use self::settings::MediaStreamSettings;
