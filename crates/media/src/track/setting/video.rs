use std::fmt::Debug;

mod aspect_ratio;
mod facing_mode;
mod frame_rate;
mod height;
mod resize_mode;
mod width;

pub use aspect_ratio::*;
pub use facing_mode::*;
pub use frame_rate::*;
pub use height::*;
pub use resize_mode::*;
pub use width::*;

/// A video's settings
#[derive(PartialEq, Clone)]
pub struct Video {
    pub width: Option<Width>,
    pub height: Option<Height>,
    pub aspect_ratio: Option<AspectRatio>,
    pub frame_rate: Option<FrameRate>,
    pub facing_mode: Option<FacingMode>,
    pub resize_mode: Option<ResizeMode>,
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
