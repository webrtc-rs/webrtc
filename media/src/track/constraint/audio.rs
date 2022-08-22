use std::fmt::Debug;

use derive_builder::Builder;

use crate::track::{
    constraint::{fitness::Fitness, NonNumeric, Numeric},
    setting::audio as setting,
};

use super::Merge;

pub type SampleRate = Numeric<setting::SampleRate>;
pub type SampleSize = Numeric<setting::SampleSize>;
pub type EchoCancellation = NonNumeric<setting::EchoCancellation>;
pub type AutoGainControl = NonNumeric<setting::AutoGainControl>;
pub type NoiseSuppression = NonNumeric<setting::NoiseSuppression>;
pub type Latency = Numeric<setting::Latency>;
pub type ChannelCount = Numeric<setting::ChannelCount>;

/// An audio's constraints
#[derive(PartialEq, Default, Clone, Builder)]
pub struct Audio {
    #[builder(default, setter(into, strip_option))]
    pub sample_rate: Option<SampleRate>,
    #[builder(default, setter(into, strip_option))]
    pub sample_size: Option<SampleSize>,
    #[builder(default, setter(into, strip_option))]
    pub echo_cancellation: Option<EchoCancellation>,
    #[builder(default, setter(into, strip_option))]
    pub auto_gain_control: Option<AutoGainControl>,
    #[builder(default, setter(into, strip_option))]
    pub noise_suppression: Option<NoiseSuppression>,
    #[builder(default, setter(into, strip_option))]
    pub latency: Option<Latency>,
    #[builder(default, setter(into, strip_option))]
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

        if let Some(sample_rate) = &self.sample_rate {
            builder.field("sample_rate", &sample_rate);
        }
        if let Some(sample_size) = &self.sample_size {
            builder.field("sample_size", &sample_size);
        }
        if let Some(echo_cancellation) = &self.echo_cancellation {
            builder.field("echo_cancellation", &echo_cancellation);
        }
        if let Some(auto_gain_control) = &self.auto_gain_control {
            builder.field("auto_gain_control", &auto_gain_control);
        }
        if let Some(noise_suppression) = &self.noise_suppression {
            builder.field("noise_suppression", &noise_suppression);
        }
        if let Some(latency) = &self.latency {
            builder.field("latency", &latency);
        }
        if let Some(channel_count) = &self.channel_count {
            builder.field("channel_count", &channel_count);
        }

        builder.finish()
    }
}

impl Merge for Audio {
    fn merge(&mut self, other: &Self) {
        if self.sample_rate.is_none() {
            self.sample_rate = other.sample_rate.clone();
        }
        if self.sample_size.is_none() {
            self.sample_size = other.sample_size.clone();
        }
        if self.echo_cancellation.is_none() {
            self.echo_cancellation = other.echo_cancellation.clone();
        }
        if self.auto_gain_control.is_none() {
            self.auto_gain_control = other.auto_gain_control.clone();
        }
        if self.noise_suppression.is_none() {
            self.noise_suppression = other.noise_suppression.clone();
        }
        if self.latency.is_none() {
            self.latency = other.latency.clone();
        }
        if self.channel_count.is_none() {
            self.channel_count = other.channel_count.clone();
        }
    }
}

impl Audio {
    fn fitness_distance(&self, settings: Option<&setting::Audio>) -> f64 {
        // TODO(regexident): replace with `let_else` once stabilized:
        // Tracking issue: https://github.com/rust-lang/rust/issues/87335
        let settings = match settings {
            Some(settings) => settings,
            None => {
                return 0.0;
            }
        };

        let mut fitness: f64 = 0.0;

        // TODO(regexident): Ignore unsupported constraints.

        if let Some(sample_rate) = &self.sample_rate {
            fitness += sample_rate.fitness_distance(settings.sample_rate.as_ref());
        }
        if let Some(sample_size) = &self.sample_size {
            fitness += sample_size.fitness_distance(settings.sample_size.as_ref());
        }
        if let Some(echo_cancellation) = &self.echo_cancellation {
            fitness += echo_cancellation.fitness_distance(settings.echo_cancellation.as_ref());
        }
        if let Some(auto_gain_control) = &self.auto_gain_control {
            fitness += auto_gain_control.fitness_distance(settings.auto_gain_control.as_ref());
        }
        if let Some(noise_suppression) = &self.noise_suppression {
            fitness += noise_suppression.fitness_distance(settings.noise_suppression.as_ref());
        }
        if let Some(latency) = &self.latency {
            fitness += latency.fitness_distance(settings.latency.as_ref());
        }
        if let Some(channel_count) = &self.channel_count {
            fitness += channel_count.fitness_distance(settings.channel_count.as_ref());
        }

        fitness
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
            .sample_rate(SampleRate::at_least(44_100, None))
            .auto_gain_control(AutoGainControl::exactly(setting::AutoGainControl::On))
            .channel_count(ChannelCount::within(2, 5, Some(2)))
            .build()
            .unwrap();
        assert_eq!(
            subject,
            Audio {
                sample_rate: Some(SampleRate::at_least(44_100, None)),
                sample_size: None,
                echo_cancellation: None,
                auto_gain_control: Some(AutoGainControl::exactly(setting::AutoGainControl::On)),
                noise_suppression: None,
                latency: None,
                channel_count: Some(ChannelCount::within(2, 5, Some(2))),
            }
        );
    }

    #[test]
    fn fitness_distance() {
        let constraint = Audio::builder()
            .sample_rate(SampleRate::at_least(44_100, None))
            .auto_gain_control(AutoGainControl::exactly(setting::AutoGainControl::On))
            .channel_count(ChannelCount::within(2, 5, Some(2)))
            .build()
            .unwrap();

        let setting = setting::Audio::builder()
            .sample_rate(44_100)
            .auto_gain_control(setting::AutoGainControl::On)
            .channel_count(4)
            .build()
            .unwrap();

        assert_eq!(constraint.fitness_distance(Some(&setting)), 0.5);
    }
}
