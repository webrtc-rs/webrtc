use std::fmt::Debug;

use crate::track::setting::NumericSetting;

/// The height or height range, in pixels.
///
/// As a capability, the range should span the video source's pre-set height
/// values with min being equal to 1 and max being the largest height.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-height>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Height(u32);

impl Height {
    pub fn from_pixels(pixels: u32) -> Self {
        assert!(pixels > 0);

        Self(pixels)
    }
}

impl From<u32> for Height {
    fn from(int: u32) -> Self {
        Self::from_pixels(int)
    }
}

impl NumericSetting for Height {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for Height {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} px", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEIGHT: u32 = 30;

    #[test]
    fn from_id() {
        let subject = Height::from_pixels(HEIGHT);
        assert_eq!(subject.0, HEIGHT);
    }

    #[test]
    fn from() {
        let subject = Height::from(HEIGHT);
        assert_eq!(subject.0, HEIGHT);
    }

    #[test]
    fn debug() {
        let subject = Height(HEIGHT);
        assert_eq!(format!("{:?}", subject), "30 px");
    }
}
