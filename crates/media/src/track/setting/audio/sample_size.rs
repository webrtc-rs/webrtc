use std::fmt::Debug;

use crate::track::setting::NumericSetting;

/// The linear sample size in bits.
///
/// This constraint can only be satisfied for audio devices that produce linear samples.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-framerate>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct SampleSize(u32);

impl SampleSize {
    pub fn from_bits(bits: u32) -> Self {
        Self(bits)
    }
}

impl From<u32> for SampleSize {
    fn from(int: u32) -> Self {
        Self::from_bits(int)
    }
}

impl NumericSetting for SampleSize {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for SampleSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} bits", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SIZE: u32 = 30;

    #[test]
    fn from_id() {
        let subject = SampleSize::from_bits(SAMPLE_SIZE);
        assert_eq!(subject.0, SAMPLE_SIZE);
    }

    #[test]
    fn from() {
        let subject = SampleSize::from(SAMPLE_SIZE);
        assert_eq!(subject.0, SAMPLE_SIZE);
    }

    #[test]
    fn debug() {
        let subject = SampleSize(SAMPLE_SIZE);
        assert_eq!(format!("{:?}", subject), "30 bits");
    }
}
