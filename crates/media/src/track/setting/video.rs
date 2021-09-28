use std::fmt::Debug;

use crate::track::setting::NumericSetting;

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

/// The width or width range, in pixels.
///
/// As a capability, the range should span the video source's pre-set width
/// values with min being equal to 1 and max being the largest width.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-width>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Width(i32);

impl Width {
    pub fn from_pixels(pixels: i32) -> Self {
        assert!(pixels > 0);

        Self(pixels)
    }
}

impl NumericSetting for Width {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for Width {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} px", self.0)
    }
}

/// The height or height range, in pixels.
///
/// As a capability, the range should span the video source's pre-set height
/// values with min being equal to 1 and max being the largest height.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-height>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Height(i32);

impl Height {
    pub fn from_pixels(pixels: i32) -> Self {
        assert!(pixels > 0);

        Self(pixels)
    }
}

impl NumericSetting for Height {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for Height {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} px", self.0)
    }
}

/// The exact aspect ratio (width in pixels divided by height in pixels,
/// represented as a double rounded to the tenth decimal place) or aspect ratio range.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-aspectratio>
#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub struct AspectRatio(f64);

impl AspectRatio {
    pub fn from_ratio(ratio: f64) -> Self {
        assert!(ratio > 0.0);

        Self(ratio)
    }
}

impl NumericSetting for AspectRatio {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for AspectRatio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.0)
    }
}

#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub struct FrameRate(f64);

impl FrameRate {
    pub fn from_hertz(hz: f64) -> Self {
        assert!(hz > 0.0);

        Self(hz)
    }
}

impl NumericSetting for FrameRate {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for FrameRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} fps", self.0)
    }
}

/// The directions that the camera can face, as seen from the user's perspective.
///
/// # Important
/// Note that `getConstraints` may not return exactly the same string for strings not in this enum.
/// This preserves the possibility of using a future version of WebIDL enum for this setting.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-framerate>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum FacingMode {
    /// The source is facing toward the user (a self-view camera).
    User,

    /// The source is facing away from the user (viewing the environment).
    Environment,

    /// The source is facing to the left of the user.
    Left,

    /// The source is facing to the right of the user.
    Right,
}

impl Debug for FacingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => writeln!(f, "user"),
            Self::Environment => writeln!(f, "environment"),
            Self::Left => writeln!(f, "left"),
            Self::Right => writeln!(f, "right"),
        }
    }
}

/// The directions that the camera can face, as seen from the user's perspective.
///
/// # Important
/// Note that `getConstraints` may not return exactly the same string for strings not in this enum.
/// This preserves the possibility of using a future version of WebIDL enum for this setting.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-resizemode>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum ResizeMode {
    /// This resolution and frame rate is offered by the camera, its driver, or the OS.
    None,

    /// This resolution is down-scaled and/or cropped from a higher camera resolution by the User Agent,
    /// or its frame rate is decimated by the User Agent.
    /// The media MUST NOT be up-scaled, stretched or have fake data
    /// created that did not occur in the input source.
    CropAndScale,
}

impl Debug for ResizeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => writeln!(f, "none"),
            Self::CropAndScale => writeln!(f, "crop and scale"),
        }
    }
}
