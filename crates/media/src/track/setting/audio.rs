use std::fmt::Debug;

mod auto_gain_control;
mod channel_count;
mod echo_cancellation;
mod latency;
mod noise_suppression;
mod sample_rate;
mod sample_size;

pub use auto_gain_control::*;
pub use channel_count::*;
pub use echo_cancellation::*;
pub use latency::*;
pub use noise_suppression::*;
pub use sample_rate::*;
pub use sample_size::*;

/// An audio's settings
#[derive(PartialEq, Clone)]
pub struct Audio {
    pub sample_rate: Option<SampleRate>,
    pub sample_size: Option<SampleSize>,
    pub echo_cancellation: Option<EchoCancellation>,
    pub auto_gain_control: Option<AutoGainControl>,
    pub noise_suppression: Option<NoiseSuppression>,
    pub latency: Option<Latency>,
    pub channel_count: Option<ChannelCount>,
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
