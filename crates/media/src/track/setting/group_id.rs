//! Media track "group id" setting.

use std::fmt::Debug;

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
    pub fn from_id<T: Into<String>>(id: T) -> Self {
        Self(id.into())
    }
}

impl<T: AsRef<str>> From<T> for GroupId {
    fn from(string: T) -> Self {
        Self::from_id(string.as_ref())
    }
}

impl Debug for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUP_ID: &'static str = "GROUP_ID";

    #[test]
    fn from_id() {
        let group_id = GroupId::from_id(GROUP_ID);
        assert_eq!(group_id.0, GROUP_ID);
    }

    #[test]
    fn from() {
        let group_id = GroupId::from(GROUP_ID);
        assert_eq!(group_id.0, GROUP_ID);
    }

    #[test]
    fn debug() {
        let group_id = GroupId::from_id(GROUP_ID);
        assert_eq!(format!("{:?}", group_id), "GROUP_ID");
    }
}
