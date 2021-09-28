use std::fmt::Debug;

use crate::track::setting::NumericSetting;

/// The sample rate in samples per second for the audio data.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-samplerate>
#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub struct SampleRate(f64);

impl SampleRate {
    pub fn from_hertz(hz: f64) -> Self {
        Self(hz)
    }
}

impl From<f64> for SampleRate {
    fn from(float: f64) -> Self {
        Self::from_hertz(float)
    }
}

impl NumericSetting for SampleRate {
    fn float_value(&self) -> f64 {
        self.0
    }
}

impl Debug for SampleRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} sps", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: f64 = 30.0;

    #[test]
    fn from_id() {
        let subject = SampleRate::from_hertz(SAMPLE_RATE);
        assert_eq!(subject.0, SAMPLE_RATE);
    }

    #[test]
    fn from() {
        let subject = SampleRate::from(SAMPLE_RATE);
        assert_eq!(subject.0, SAMPLE_RATE);
    }

    #[test]
    fn debug() {
        let subject = SampleRate(SAMPLE_RATE);
        assert_eq!(format!("{:?}", subject), "30 sps");
    }
}
