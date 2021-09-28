//! Media track settings.

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
mod device_id;
mod group_id;

pub use device_id::*;
pub use group_id::*;

#[derive(PartialEq, Clone)]
enum MediaKind {
    Audio(audio::Audio),
    Video(video::Video),
}
