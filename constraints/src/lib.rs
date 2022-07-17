//! Pure Rust implementation of the constraint logic defined in the ["Media Capture and Streams"][mediacapture_streams] spec.
//!
//! [mediacapture_streams]: https://www.w3.org/TR/mediacapture-streams/

pub mod errors;
pub mod property;

mod capabilities;
mod capability;
mod constraint;
mod constraints;
mod macros;
mod setting;
mod settings;
mod supported_constraints;

#[allow(unused_imports)]
pub use self::{
    capabilities::MediaTrackCapabilities,
    capability::MediaTrackCapability,
    constraint::{
        BareOrMediaTrackConstraint, BareOrValueConstraint, BareOrValueRangeConstraint,
        BareOrValueSequenceConstraint, MediaTrackConstraint,
        MediaTrackConstraintResolutionStrategy, ValueConstraint, ValueRangeConstraint,
        ValueSequenceConstraint,
    },
    constraints::{
        AdvancedMediaTrackConstraints, BareOrAdvancedMediaTrackConstraints,
        BareOrBoolOrMediaTrackConstraints, BareOrMediaStreamConstraints,
        BareOrMediaTrackConstraintSet, BareOrMediaTrackConstraints, BoolOrMediaTrackConstraints,
        MediaStreamConstraints, MediaTrackConstraintSet, MediaTrackConstraints,
    },
    setting::MediaTrackSetting,
    settings::MediaTrackSettings,
    supported_constraints::MediaTrackSupportedConstraints,
};

#[allow(unused_imports)]
pub(crate) use self::{capabilities::MediaStreamCapabilities, settings::MediaStreamSettings};
