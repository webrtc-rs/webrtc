//! Constants identifying the properties of a [`MediaStreamTrack`][media_stream_track] object,
//! as defined in the ["Media Capture and Streams"][media_track_supported_constraints] spec.
//!
//! [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#mediastreamtrack
//! [media_track_supported_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatracksupportedconstraints

use std::{borrow::Cow, fmt::Display};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// An identifier for a media track property.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct MediaTrackProperty(Cow<'static, str>);

impl From<&MediaTrackProperty> for MediaTrackProperty {
    fn from(borrowed: &MediaTrackProperty) -> Self {
        borrowed.clone()
    }
}

impl From<String> for MediaTrackProperty {
    /// Creates a property from an owned representation of its name.
    fn from(owned: String) -> Self {
        Self(Cow::Owned(owned))
    }
}

impl From<&str> for MediaTrackProperty {
    /// Creates a property from an owned representation of its name.
    ///
    /// Use `MediaTrackProperty::named(str)` if your property name
    /// is statically borrowed (i.e. `&'static str`).
    fn from(borrowed: &str) -> Self {
        Self(Cow::Owned(borrowed.to_owned()))
    }
}

impl Display for MediaTrackProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl MediaTrackProperty {
    /// Creates a property from a statically borrowed representation of its name.
    pub const fn named(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    pub fn name(&self) -> &str {
        &self.0
    }
}

/// Properties that apply to all device types.
pub mod common {
    use super::*;

    /// Names of common properties.
    pub mod name {
        use super::*;

        /// The identifier of the device generating the content of the track,
        /// as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-deviceid
        pub static DEVICE_ID: MediaTrackProperty = MediaTrackProperty::named("deviceId");

        /// The document-unique group identifier for the device generating the content
        /// of the track, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-groupid
        pub static GROUP_ID: MediaTrackProperty = MediaTrackProperty::named("groupId");
    }

    /// Names of common properties.
    pub fn names() -> Vec<&'static MediaTrackProperty> {
        use self::name::*;

        vec![&DEVICE_ID, &GROUP_ID]
    }
}

/// Properties that apply only to audio device types.
pub mod audio_only {
    use super::*;

    /// Names of audio-only properties.
    pub mod name {
        use super::*;

        /// Automatic gain control is often desirable on the input signal recorded
        /// by the microphone, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-autogaincontrol
        pub static AUTO_GAIN_CONTROL: MediaTrackProperty =
            MediaTrackProperty::named("autoGainControl");

        /// The number of independent channels of sound that the audio data contains,
        /// i.e. the number of audio samples per sample frame, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-channelcount
        pub static CHANNEL_COUNT: MediaTrackProperty = MediaTrackProperty::named("channelCount");

        /// When one or more audio streams is being played in the processes of
        /// various microphones, it is often desirable to attempt to remove
        /// all the sound being played from the input signals recorded by the microphones.
        /// This is referred to as echo cancellation, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-echocancellation
        pub static ECHO_CANCELLATION: MediaTrackProperty =
            MediaTrackProperty::named("echoCancellation");

        /// The latency or latency range, in seconds, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-latency
        pub static LATENCY: MediaTrackProperty = MediaTrackProperty::named("latency");

        /// Noise suppression is often desirable on the input signal recorded by the microphone,
        /// as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-noisesuppression
        pub static NOISE_SUPPRESSION: MediaTrackProperty =
            MediaTrackProperty::named("noiseSuppression");

        /// The sample rate in samples per second for the audio data, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-samplerate
        pub static SAMPLE_RATE: MediaTrackProperty = MediaTrackProperty::named("sampleRate");

        /// The linear sample size in bits. This constraint can only
        /// be satisfied for audio devices that produce linear samples, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-samplesize
        pub static SAMPLE_SIZE: MediaTrackProperty = MediaTrackProperty::named("sampleSize");
    }

    /// Names of all audio-only properties.
    pub fn names() -> Vec<&'static MediaTrackProperty> {
        use self::name::*;

        vec![
            &AUTO_GAIN_CONTROL,
            &CHANNEL_COUNT,
            &ECHO_CANCELLATION,
            &LATENCY,
            &NOISE_SUPPRESSION,
            &SAMPLE_RATE,
            &SAMPLE_SIZE,
        ]
    }
}

/// Properties that apply only to video device types.
pub mod video_only {
    use super::*;

    /// Names of audio-only properties.
    pub mod name {
        use super::*;

        /// The exact aspect ratio (width in pixels divided by height in pixels,
        /// represented as a double rounded to the tenth decimal place),
        /// as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-aspectratio
        pub static ASPECT_RATIO: MediaTrackProperty = MediaTrackProperty::named("aspectRatio");

        /// The directions that the camera can face, as seen from the user's perspective,
        /// as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-facingmode
        pub static FACING_MODE: MediaTrackProperty = MediaTrackProperty::named("facingMode");

        /// The exact frame rate (frames per second) or frame rate range,
        /// as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-framerate
        pub static FRAME_RATE: MediaTrackProperty = MediaTrackProperty::named("frameRate");

        /// The height or height range, in pixels, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-height
        pub static HEIGHT: MediaTrackProperty = MediaTrackProperty::named("height");

        /// The width or width range, in pixels, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-width
        pub static WIDTH: MediaTrackProperty = MediaTrackProperty::named("width");

        /// The means by which the resolution can be derived by the client, as defined in the [spec][spec].
        ///
        /// In other words, whether the client is allowed to use cropping and downscaling on the camera output.
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-resizemode
        pub static RESIZE_MODE: MediaTrackProperty = MediaTrackProperty::named("resizeMode");
    }

    /// Names of all video-only properties.
    pub fn names() -> Vec<&'static MediaTrackProperty> {
        use self::name::*;
        vec![
            &ASPECT_RATIO,
            &FACING_MODE,
            &FRAME_RATE,
            &HEIGHT,
            &WIDTH,
            &RESIZE_MODE,
        ]
    }
}

pub mod all {
    use super::*;

    /// Names of all properties.
    pub mod name {
        pub use super::audio_only::name::*;
        pub use super::common::name::*;
        pub use super::video_only::name::*;
    }

    /// Names of all properties.
    pub fn names() -> Vec<&'static MediaTrackProperty> {
        let mut all = vec![];
        all.append(&mut self::common::names());
        all.append(&mut self::audio_only::names());
        all.append(&mut self::video_only::names());
        all
    }
}
