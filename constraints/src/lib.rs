//! Pure Rust implementation of the constraint logic defined in the ["Media Capture and Streams"][mediacapture_streams] spec.
//!
//! [mediacapture_streams]: https://www.w3.org/TR/mediacapture-streams/

pub mod property;

mod macros;
mod supported_constraints;

#[allow(unused_imports)]
pub use self::supported_constraints::MediaTrackSupportedConstraints;
