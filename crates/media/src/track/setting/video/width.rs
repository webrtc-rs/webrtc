use std::fmt::Debug;

use crate::track::setting::NumericSetting;

/// The width or width range, in pixels.
///
/// As a capability, the range should span the video source's pre-set width
/// values with min being equal to 1 and max being the largest width.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-width>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Width(u32);

impl Width {
    pub fn from_pixels(pixels: u32) -> Self {
        assert!(pixels > 0);

        Self(pixels)
    }
}

impl From<u32> for Width {
    fn from(int: u32) -> Self {
        Self::from_pixels(int)
    }
}

impl NumericSetting for Width {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for Width {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} px", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: u32 = 30;

    #[test]
    fn from_id() {
        let subject = Width::from_pixels(WIDTH);
        assert_eq!(subject.0, WIDTH);
    }

    #[test]
    fn from() {
        let subject = Width::from(WIDTH);
        assert_eq!(subject.0, WIDTH);
    }

    #[test]
    fn debug() {
        let subject = Width(WIDTH);
        assert_eq!(format!("{:?}", subject), "30 px");
    }
}
