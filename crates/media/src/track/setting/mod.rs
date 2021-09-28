//! Media track settings.

use std::fmt::Debug;

use derive_builder::Builder;

pub mod audio;
pub mod video;

mod device_id;
mod group_id;

pub use device_id::*;
pub use group_id::*;

pub(crate) trait NumericSetting {
    fn float_value(&self) -> f64;
}

#[derive(PartialEq, Clone)]
pub enum MediaKind {
    Audio(audio::Audio),
    Video(video::Video),
}

impl From<audio::Audio> for MediaKind {
    fn from(audio: audio::Audio) -> Self {
        Self::Audio(audio)
    }
}

impl From<video::Video> for MediaKind {
    fn from(video: video::Video) -> Self {
        Self::Video(video)
    }
}

impl Debug for MediaKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Audio(audio) => audio.fmt(f),
            Self::Video(video) => video.fmt(f),
        }
    }
}

#[derive(PartialEq, Clone, Builder)]
pub struct Media {
    #[builder(default, setter(into, strip_option))]
    pub device_id: Option<DeviceId>,
    #[builder(default, setter(into, strip_option))]
    pub group_id: Option<GroupId>,
    #[builder(setter(into))]
    pub kind: MediaKind,
}

impl Media {
    pub fn builder() -> MediaBuilder {
        Default::default()
    }

    pub fn new(device_id: Option<DeviceId>, group_id: Option<GroupId>, kind: MediaKind) -> Self {
        Self {
            device_id,
            group_id,
            kind,
        }
    }
}

impl Debug for Media {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut builder = f.debug_struct("Media");

        if let Some(device_id) = &self.device_id {
            builder.field("device_id", &device_id);
        }
        if let Some(group_id) = &self.group_id {
            builder.field("group_id", &group_id);
        }

        builder.field("kind", &self.kind);

        builder.finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::track::setting::video::{Video, Width};

    use super::*;

    #[test]
    fn builder() {
        let subject = Media::builder()
            .device_id(DeviceId::from("DEVICE"))
            .kind(Video::default())
            .build()
            .unwrap();
        assert_eq!(
            subject,
            Media {
                device_id: Some(DeviceId::from("DEVICE")),
                group_id: None,
                kind: MediaKind::Video(Video::default())
            }
        );
    }

    #[test]
    fn debug() {
        let subject = Media {
            device_id: Some(DeviceId::from("DEVICE")),
            group_id: None,
            kind: MediaKind::Video(Video {
                width: Some(Width::from(42)),
                height: None,
                aspect_ratio: None,
                frame_rate: None,
                facing_mode: None,
                resize_mode: None,
            }),
        };
        assert_eq!(
            format!("{:?}", subject),
            "Media { device_id: DEVICE, kind: Video { width: 42 px } }"
        );
    }
}
