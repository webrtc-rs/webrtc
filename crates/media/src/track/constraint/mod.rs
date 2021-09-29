//! Media track constraints.

pub mod audio;
pub mod video;

mod non_numeric;
mod numeric;

mod fitness;

pub use non_numeric::*;
pub use numeric::*;

pub(crate) use fitness::*;

type DeviceId = NonNumeric<String>;
type GroupId = NonNumeric<String>;

pub trait Merge {
    fn merge(&mut self, other: &Self);
}

#[derive(PartialEq, Clone, Debug)]
enum MediaKind {
    Audio(audio::Audio),
    Video(video::Video),
}

#[derive(PartialEq, Clone, Debug)]
struct Media {
    pub device_id: Option<DeviceId>,
    pub group_id: Option<GroupId>,
    pub kind: MediaKind,
}

impl Merge for Media {
    fn merge(&mut self, other: &Self) {
        if self.device_id.is_none() {
            self.device_id = other.device_id.clone();
        }
        if self.group_id.is_none() {
            self.group_id = other.group_id.clone();
        }
        match (&mut self.kind, &other.kind) {
            (MediaKind::Audio(lhs), MediaKind::Audio(rhs)) => lhs.merge(rhs),
            (MediaKind::Video(lhs), MediaKind::Video(rhs)) => lhs.merge(rhs),
            (MediaKind::Video(_), MediaKind::Audio(_)) => {
                eprintln!("Cannot merge video constraints with audio constraints")
            }
            (MediaKind::Audio(_), MediaKind::Video(_)) => {
                eprintln!("Cannot merge audio constraints with video constraints")
            }
        }
    }
}
