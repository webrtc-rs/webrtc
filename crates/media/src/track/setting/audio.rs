use std::fmt::Debug;

use crate::track::setting::NumericSetting;

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

impl NumericSetting for SampleRate {
    fn float_value(&self) -> f64 {
        self.0
    }
}

impl Debug for SampleRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} sps", self.0)
    }
}

/// The linear sample size in bits.
///
/// This constraint can only be satisfied for audio devices that produce linear samples.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-framerate>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct SampleSize(i64);

impl SampleSize {
    pub fn from_bits(bits: i64) -> Self {
        Self(bits)
    }
}

impl NumericSetting for SampleSize {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for SampleSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} bits", self.0)
    }
}

/// When one or more audio streams is being played in the processes of various microphones,
/// it is often desirable to attempt to remove all the sound being played from the input signals
/// recorded by the microphones. This is referred to as echo cancellation.
///
/// There are cases where it is not needed and it is desirable to turn it off
/// so that no audio artifacts are introduced. This allows applications to control this behavior.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-echocancellation>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum EchoCancellation {
    Off,
    On,
}

impl From<bool> for EchoCancellation {
    fn from(boolean: bool) -> Self {
        if boolean {
            Self::On
        } else {
            Self::Off
        }
    }
}

impl From<EchoCancellation> for bool {
    fn from(echo_cancellation: EchoCancellation) -> Self {
        match echo_cancellation {
            EchoCancellation::Off => false,
            EchoCancellation::On => true,
        }
    }
}

impl Debug for EchoCancellation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => writeln!(f, "off"),
            Self::On => writeln!(f, "on"),
        }
    }
}

/// Automatic gain control is often desirable on the input signal recorded by the microphone.
///
/// There are cases where it is not needed and it is desirable to turn it off so that
/// the audio is not altered. This allows applications to control this behavior.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-autogaincontrol>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum AutoGainControl {
    Off,
    On,
}

impl From<bool> for AutoGainControl {
    fn from(boolean: bool) -> Self {
        if boolean {
            Self::On
        } else {
            Self::Off
        }
    }
}

impl From<AutoGainControl> for bool {
    fn from(echo_cancellation: AutoGainControl) -> Self {
        match echo_cancellation {
            AutoGainControl::Off => false,
            AutoGainControl::On => true,
        }
    }
}

impl Debug for AutoGainControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => writeln!(f, "off"),
            Self::On => writeln!(f, "on"),
        }
    }
}

/// Noise suppression is often desirable on the input signal recorded by the microphone.
///
/// There are cases where it is not needed and it is desirable to turn it off so that
/// the audio is not altered. This allows applications to control this behavior.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-noisesuppression>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum NoiseSuppression {
    Off,
    On,
}

impl From<bool> for NoiseSuppression {
    fn from(boolean: bool) -> Self {
        if boolean {
            Self::On
        } else {
            Self::Off
        }
    }
}

impl From<NoiseSuppression> for bool {
    fn from(echo_cancellation: NoiseSuppression) -> Self {
        match echo_cancellation {
            NoiseSuppression::Off => false,
            NoiseSuppression::On => true,
        }
    }
}

impl Debug for NoiseSuppression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => writeln!(f, "off"),
            Self::On => writeln!(f, "on"),
        }
    }
}

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

impl NumericSetting for Latency {
    fn float_value(&self) -> f64 {
        self.0
    }
}

impl Debug for Latency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} sec", self.0)
    }
}

/// The number of independent channels of sound that the audio data contains,
/// i.e. the number of audio samples per sample frame.
///
/// # Specification
/// - <https://www.w3.org/TR/mediacapture-streams/#dfn-channelcount>
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct ChannelCount(i64);

impl ChannelCount {
    pub fn from_channels(channels: i64) -> Self {
        assert!(channels > 0);

        Self(channels)
    }
}

impl NumericSetting for ChannelCount {
    fn float_value(&self) -> f64 {
        self.0 as f64
    }
}

impl Debug for ChannelCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} channels", self.0)
    }
}
