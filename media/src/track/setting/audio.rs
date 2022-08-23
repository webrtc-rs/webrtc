use std::fmt::Debug;

use derive_builder::Builder;

mod auto_gain_control;
mod echo_cancellation;
mod noise_suppression;

pub use auto_gain_control::*;
pub use echo_cancellation::*;
pub use noise_suppression::*;

pub type SampleRate = u32;
pub type SampleSize = u32;
pub type Latency = f64;
pub type ChannelCount = u32;

/// An audio's settings
#[derive(PartialEq, Default, Clone, Builder)]
pub struct Audio {
    #[builder(default, setter(strip_option))]
    pub sample_rate: Option<SampleRate>,
    #[builder(default, setter(strip_option))]
    pub sample_size: Option<SampleSize>,
    #[builder(default, setter(strip_option))]
    pub echo_cancellation: Option<EchoCancellation>,
    #[builder(default, setter(strip_option))]
    pub auto_gain_control: Option<AutoGainControl>,
    #[builder(default, setter(strip_option))]
    pub noise_suppression: Option<NoiseSuppression>,
    #[builder(default, setter(strip_option))]
    pub latency: Option<Latency>,
    #[builder(default, setter(strip_option))]
    pub channel_count: Option<ChannelCount>,
}

impl Audio {
    pub fn builder() -> AudioBuilder {
        Default::default()
    }

    pub fn new(
        sample_rate: Option<SampleRate>,
        sample_size: Option<SampleSize>,
        echo_cancellation: Option<EchoCancellation>,
        auto_gain_control: Option<AutoGainControl>,
        noise_suppression: Option<NoiseSuppression>,
        latency: Option<Latency>,
        channel_count: Option<ChannelCount>,
    ) -> Self {
        Self {
            sample_rate,
            sample_size,
            echo_cancellation,
            auto_gain_control,
            noise_suppression,
            latency,
            channel_count,
        }
    }
}

impl Debug for Audio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut builder = f.debug_struct("Audio");

        if let Some(sample_rate) = self.sample_rate {
            builder.field("sample_rate", &sample_rate);
        }
        if let Some(sample_size) = self.sample_size {
            builder.field("sample_size", &sample_size);
        }
        if let Some(echo_cancellation) = self.echo_cancellation {
            builder.field("echo_cancellation", &echo_cancellation);
        }
        if let Some(auto_gain_control) = self.auto_gain_control {
            builder.field("auto_gain_control", &auto_gain_control);
        }
        if let Some(noise_suppression) = self.noise_suppression {
            builder.field("noise_suppression", &noise_suppression);
        }
        if let Some(latency) = self.latency {
            builder.field("latency", &latency);
        }
        if let Some(channel_count) = self.channel_count {
            builder.field("channel_count", &channel_count);
        }

        builder.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default() {
        let subject = Audio::default();
        assert_eq!(
            subject,
            Audio {
                sample_rate: None,
                sample_size: None,
                echo_cancellation: None,
                auto_gain_control: None,
                noise_suppression: None,
                latency: None,
                channel_count: None,
            }
        );
    }

    #[test]
    fn builder() {
        let subject = Audio::builder()
            .sample_rate(44_100)
            .auto_gain_control(AutoGainControl::On)
            .channel_count(42)
            .build()
            .unwrap();
        assert_eq!(
            subject,
            Audio {
                sample_rate: Some(44_100),
                sample_size: None,
                echo_cancellation: None,
                auto_gain_control: Some(AutoGainControl::On),
                noise_suppression: None,
                latency: None,
                channel_count: Some(42),
            }
        );
    }

    #[test]
    fn debug() {
        let subject = Audio {
            sample_rate: Some(44_100),
            sample_size: None,
            echo_cancellation: None,
            auto_gain_control: Some(AutoGainControl::On),
            noise_suppression: None,
            latency: None,
            channel_count: Some(42),
        };
        assert_eq!(
            format!("{:?}", subject),
            "Audio { sample_rate: 44100, auto_gain_control: On, channel_count: 42 }"
        );
    }

    #[test]
    fn debug_empty() {
        let subject = Audio::default();
        assert_eq!(format!("{:?}", subject), "Audio");
    }
}
