//! Media track settings.

use std::fmt::Debug;

pub mod audio;
pub mod video;

pub(crate) trait NumericSetting {
    fn float_value(&self) -> f64;
}

#[derive(PartialEq, Clone)]
struct Media {
    pub device_id: Option<DeviceId>,
    pub group_id: Option<GroupId>,
    pub kind: MediaKind,
}

#[derive(PartialEq, Clone)]
enum MediaKind {
    Audio(audio::Audio),
    Video(video::Video),
}

/// The identifier of the device generating the content of the MediaStreamTrack.
///
/// It conforms with the definition of `MediaDeviceInfo.deviceId`.
///
/// # Note
///
/// Note that the setting of this setting is uniquely determined by
/// the source that is attached to the MediaStreamTrack.
/// In particular, getCapabilities() will return only a single value for `deviceId`.
/// This setting can therefore be used for initial media selection with `getUserMedia()`.
/// However, it is not useful for subsequent media control with `applyConstraints()`,
/// since any attempt to set a different value will result in an unsatisfiable `ConstraintSet`.
/// If a string of length `0` is used as a `deviceId` value constraint with `getUserMedia()`,
/// it MAY be interpreted as if the constraint is not specified.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-deviceid>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn string(string: String) -> Self {
        Self(string)
    }
}

impl Debug for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.0)
    }
}

/// The document-unique group identifier for the device generating the content of the MediaStreamTrack.
///
/// It conforms with the definition of `MediaDeviceInfo.groupId`.
///
/// # Note
/// Note that the setting of this setting is uniquely determined by the source that is attached to the MediaStreamTrack.
/// In particular, `getCapabilities()` will return only a single value for `groupId`.
/// Since this setting is not stable between browsing sessions,
/// its usefulness for initial media selection with `getUserMedia()` is limited.
/// It is not useful for subsequent media control with `applyConstraints()`,
/// since any attempt to set a different value will result in an unsatisfiable `ConstraintSet`.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-groupid>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct GroupId(String);

impl GroupId {
    pub fn string(string: String) -> Self {
        Self(string)
    }
}

impl Debug for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.0)
    }
}
