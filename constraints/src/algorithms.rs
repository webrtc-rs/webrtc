//! Algorithms as defined in the ["Media Capture and Streams"][mediacapture_streams] spec.
//!
//! [mediacapture_streams]: https://www.w3.org/TR/mediacapture-streams/

mod fitness_distance;
mod select_settings;

pub use self::fitness_distance::*;
pub use self::select_settings::*;
