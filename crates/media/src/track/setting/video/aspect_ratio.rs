use std::fmt::Debug;

use crate::track::setting::NumericSetting;

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

impl From<f64> for AspectRatio {
    fn from(float: f64) -> Self {
        Self::from_ratio(float)
    }
}

impl NumericSetting for AspectRatio {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for AspectRatio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASPECT_RATIO: f64 = 1.5;

    #[test]
    fn from_id() {
        let subject = AspectRatio::from_ratio(ASPECT_RATIO);
        assert_eq!(subject.0, ASPECT_RATIO);
    }

    #[test]
    fn from() {
        let subject = AspectRatio::from(ASPECT_RATIO);
        assert_eq!(subject.0, ASPECT_RATIO);
    }

    #[test]
    fn debug() {
        let subject = AspectRatio(ASPECT_RATIO);
        assert_eq!(format!("{:?}", subject), "1.5");
    }
}
