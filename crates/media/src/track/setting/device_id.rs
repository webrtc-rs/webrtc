//! Media track "device id" setting.

use std::fmt::Debug;

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
    pub fn from_id<T: Into<String>>(id: T) -> Self {
        Self(id.into())
    }
}

impl<T: AsRef<str>> From<T> for DeviceId {
    fn from(string: T) -> Self {
        Self::from_id(string.as_ref())
    }
}

impl Debug for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICE_ID: &'static str = "DEVICE_ID";

    #[test]
    fn from_id() {
        let device_id = DeviceId::from_id(DEVICE_ID);
        assert_eq!(device_id.0, DEVICE_ID);
    }

    #[test]
    fn from() {
        let device_id = DeviceId::from(DEVICE_ID);
        assert_eq!(device_id.0, DEVICE_ID);
    }

    #[test]
    fn debug() {
        let device_id = DeviceId::from_id(DEVICE_ID);
        assert_eq!(format!("{:?}", device_id), "DEVICE_ID");
    }
}
