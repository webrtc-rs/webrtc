use std::fmt::Debug;

use crate::track::setting::NumericSetting;

/// The latency or latency range, in seconds.
///
/// The latency is the time between start of processing
/// (for instance, when sound occurs in the real world)
/// to the data being available to the next step in the process.
///
/// Low latency is critical for some applications;
/// high latency may be acceptable for other applications because it helps with power constraints.
///
/// The number is expected to be the target latency of the configuration;
/// the actual latency may show some variation from that.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-latency>
#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub struct Latency(f64);

impl Latency {
    pub fn from_seconds(seconds: f64) -> Self {
        assert!(seconds >= 0.0);

        Self(seconds)
    }
}

impl From<f64> for Latency {
    fn from(float: f64) -> Self {
        Self::from_seconds(float)
    }
}

impl NumericSetting for Latency {
    fn float_value(&self) -> f64 {
        self.0
    }
}

impl Debug for Latency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} sec", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LATENCY: f64 = 30.0;

    #[test]
    fn from_id() {
        let subject = Latency::from_seconds(LATENCY);
        assert_eq!(subject.0, LATENCY);
    }

    #[test]
    fn from() {
        let subject = Latency::from(LATENCY);
        assert_eq!(subject.0, LATENCY);
    }

    #[test]
    fn debug() {
        let subject = Latency(LATENCY);
        assert_eq!(format!("{:?}", subject), "30 sec");
    }
}
