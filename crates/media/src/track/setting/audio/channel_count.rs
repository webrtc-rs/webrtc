use std::fmt::Debug;

use crate::track::setting::NumericSetting;

/// The number of independent channels of sound that the audio data contains,
/// i.e. the number of audio samples per sample frame.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-channelcount>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct ChannelCount(u32);

impl ChannelCount {
    pub fn from_channels(channels: u32) -> Self {
        assert!(channels > 0);

        Self(channels)
    }
}

impl From<u32> for ChannelCount {
    fn from(int: u32) -> Self {
        Self::from_channels(int)
    }
}

impl NumericSetting for ChannelCount {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for ChannelCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANNEL_COUNT: u32 = 30;

    #[test]
    fn from_id() {
        let subject = ChannelCount::from_channels(CHANNEL_COUNT);
        assert_eq!(subject.0, CHANNEL_COUNT);
    }

    #[test]
    fn from() {
        let subject = ChannelCount::from(CHANNEL_COUNT);
        assert_eq!(subject.0, CHANNEL_COUNT);
    }

    #[test]
    fn debug() {
        let subject = ChannelCount(CHANNEL_COUNT);
        assert_eq!(format!("{:?}", subject), "30");
    }
}
