use std::fmt::Debug;

use derive_builder::Builder;

mod facing_mode;
mod resize_mode;

pub use facing_mode::*;
pub use resize_mode::*;

pub type Width = u32;
pub type Height = u32;
pub type AspectRatio = f64;
pub type FrameRate = f64;

/// A video's settings
#[derive(PartialEq, Default, Clone, Builder)]
pub struct Video {
    #[builder(default, setter(strip_option))]
    pub width: Option<Width>,
    #[builder(default, setter(strip_option))]
    pub height: Option<Height>,
    #[builder(default, setter(strip_option))]
    pub aspect_ratio: Option<AspectRatio>,
    #[builder(default, setter(strip_option))]
    pub frame_rate: Option<FrameRate>,
    #[builder(default, setter(strip_option))]
    pub facing_mode: Option<FacingMode>,
    #[builder(default, setter(strip_option))]
    pub resize_mode: Option<ResizeMode>,
}

impl Video {
    pub fn builder() -> VideoBuilder {
        Default::default()
    }

    pub fn new(
        width: Option<Width>,
        height: Option<Height>,
        aspect_ratio: Option<AspectRatio>,
        frame_rate: Option<FrameRate>,
        facing_mode: Option<FacingMode>,
        resize_mode: Option<ResizeMode>,
    ) -> Self {
        Self {
            width,
            height,
            aspect_ratio,
            frame_rate,
            facing_mode,
            resize_mode,
        }
    }
}

impl Debug for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut builder = f.debug_struct("Video");

        if let Some(width) = self.width {
            builder.field("width", &width);
        }
        if let Some(height) = self.height {
            builder.field("height", &height);
        }
        if let Some(aspect_ratio) = self.aspect_ratio {
            builder.field("aspect_ratio", &aspect_ratio);
        }
        if let Some(frame_rate) = self.frame_rate {
            builder.field("frame_rate", &frame_rate);
        }
        if let Some(facing_mode) = self.facing_mode {
            builder.field("facing_mode", &facing_mode);
        }
        if let Some(resize_mode) = self.resize_mode {
            builder.field("resize_mode", &resize_mode);
        }

        builder.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default() {
        let subject = Video::default();
        assert_eq!(
            subject,
            Video {
                width: None,
                height: None,
                aspect_ratio: None,
                frame_rate: None,
                facing_mode: None,
                resize_mode: None,
            }
        );
    }

    #[test]
    fn builder() {
        let subject = Video::builder()
            .height(42)
            .frame_rate(30.0)
            .resize_mode(ResizeMode::CropAndScale)
            .build()
            .unwrap();
        assert_eq!(
            subject,
            Video {
                width: None,
                height: Some(42),
                aspect_ratio: None,
                frame_rate: Some(30.0),
                facing_mode: None,
                resize_mode: Some(ResizeMode::CropAndScale),
            }
        );
    }

    #[test]
    fn debug() {
        let subject = Video {
            width: None,
            height: Some(42),
            aspect_ratio: None,
            frame_rate: Some(30.0),
            facing_mode: None,
            resize_mode: Some(ResizeMode::CropAndScale),
        };
        assert_eq!(
            format!("{:?}", subject),
            "Video { height: 42, frame_rate: 30.0, resize_mode: CropAndScale }"
        );
    }

    #[test]
    fn debug_empty() {
        let subject = Video::default();
        assert_eq!(format!("{:?}", subject), "Video");
    }
}
