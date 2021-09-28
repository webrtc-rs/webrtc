use std::fmt::Debug;

use crate::track::setting::NumericSetting;

/// The exact frame rate (frames per second) or frame rate range.
///
/// If video source's pre-set can determine frame rate values,
/// the range, as a capacity, should span the video source's pre-set
// frame rate values with `min` being equal to `0` and `max`
// being the largest frame rate.
///
/// # Important
/// The User Agent MUST support frame rates obtained from integral decimation
/// of the native resolution frame rate. If this frame rate cannot be determined
/// (e.g. the source does not natively provide a frame rate, or the frame rate
/// cannot be determined from the source stream),
/// then this value MUST refer to the User Agent's vsync display rate.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-framerate>
#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub struct FrameRate(f64);

impl FrameRate {
    pub fn from_hertz(hz: f64) -> Self {
        assert!(hz > 0.0);

        Self(hz)
    }
}

impl From<f64> for FrameRate {
    fn from(float: f64) -> Self {
        Self::from_hertz(float)
    }
}

impl NumericSetting for FrameRate {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for FrameRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} fps", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME_RATE: f64 = 30.0;

    #[test]
    fn from_id() {
        let subject = FrameRate::from_hertz(FRAME_RATE);
        assert_eq!(subject.0, FRAME_RATE);
    }

    #[test]
    fn from() {
        let subject = FrameRate::from(FRAME_RATE);
        assert_eq!(subject.0, FRAME_RATE);
    }

    #[test]
    fn debug() {
        let subject = FrameRate(FRAME_RATE);
        assert_eq!(format!("{:?}", subject), "30 fps");
    }
}
