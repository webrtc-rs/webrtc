/// The directions that the camera can face, as seen from the user's perspective.
///
/// # Note
/// The enumeration is not exhaustive and merely provides a list of known values.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
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

impl FacingMode {
    /// Returns `"user"`, the string-value of the `User` facing mode.
    pub fn user() -> String {
        Self::User.to_string()
    }

    /// Returns `"environment"`, the string-value of the `Environment` facing mode.
    pub fn environment() -> String {
        Self::Environment.to_string()
    }

    /// Returns `"left"`, the string-value of the `Left` facing mode.
    pub fn left() -> String {
        Self::Left.to_string()
    }

    /// Returns `"right"`, the string-value of the `Right` facing mode.
    pub fn right() -> String {
        Self::Right.to_string()
    }
}

impl std::fmt::Display for FacingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => f.write_str("user"),
            Self::Environment => f.write_str("environment"),
            Self::Left => f.write_str("left"),
            Self::Right => f.write_str("right"),
        }
    }
}

/// The means by which the resolution can be derived by the client.
///
/// # Note
/// The enumeration is not exhaustive and merely provides a list of known values.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ResizeMode {
    /// This resolution and frame rate is offered by the camera, its driver, or the OS.
    ///
    /// # Note
    /// The user agent MAY report this value to disguise concurrent use,
    /// but only when the camera is in use in another browsing context.
    ///
    /// # Important
    /// This value is a possible finger-printing surface.
    None,

    /// This resolution is downscaled and/or cropped from a higher camera resolution by the user agent,
    /// or its frame rate is decimated by the User Agent.
    ///
    /// # Important
    /// The media MUST NOT be upscaled, stretched or have fake data created that did not occur in the input source.
    CropAndScale,
}

impl ResizeMode {
    /// Returns `"none"`, the string-value of the `None` resize mode.
    pub fn none() -> String {
        Self::None.to_string()
    }

    /// Returns `"crop-and-scale"`, the string-value of the `CropAndScale` resize mode.
    pub fn crop_and_scale() -> String {
        Self::CropAndScale.to_string()
    }
}

impl std::fmt::Display for ResizeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("none"),
            Self::CropAndScale => f.write_str("crop-and-scale"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod facing_mode {
        use super::*;

        #[test]
        fn to_string() {
            assert_eq!(FacingMode::User.to_string(), "user");
            assert_eq!(FacingMode::Environment.to_string(), "environment");
            assert_eq!(FacingMode::Left.to_string(), "left");
            assert_eq!(FacingMode::Right.to_string(), "right");

            assert_eq!(FacingMode::user(), "user");
            assert_eq!(FacingMode::environment(), "environment");
            assert_eq!(FacingMode::left(), "left");
            assert_eq!(FacingMode::right(), "right");
        }
    }

    mod resize_mode {
        use super::*;

        #[test]
        fn to_string() {
            assert_eq!(ResizeMode::None.to_string(), "none");
            assert_eq!(ResizeMode::CropAndScale.to_string(), "crop-and-scale");

            assert_eq!(ResizeMode::none(), "none");
            assert_eq!(ResizeMode::crop_and_scale(), "crop-and-scale");
        }
    }
}
