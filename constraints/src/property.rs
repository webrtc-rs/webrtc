//! Constants identifying the properties of a [`MediaStreamTrack`][media_stream_track] object,
//! as defined in the ["Media Capture and Streams"][media_track_supported_constraints] spec.
//!
//! [media_stream_track]: https://www.w3.org/TR/mediacapture-streams/#mediastreamtrack
//! [media_track_supported_constraints]: https://www.w3.org/TR/mediacapture-streams/#dom-mediatracksupportedconstraints

/// Properties that apply to all device types.
pub mod common {
    /// Names of common properties.
    pub mod name {
        /// The identifier of the device generating the content of the track,
        /// as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-deviceid
        pub const DEVICE_ID: &str = "deviceId";

        /// The document-unique group identifier for the device generating the content
        /// of the track, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-groupid
        pub const GROUP_ID: &str = "groupId";
    }

    /// Names of common properties.
    pub fn names() -> Vec<&'static str> {
        use self::name::*;

        vec![DEVICE_ID, GROUP_ID]
    }
}

/// Properties that apply only to audio device types.
pub mod audio_only {
    /// Names of audio-only properties.
    pub mod name {
        /// Automatic gain control is often desirable on the input signal recorded
        /// by the microphone, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-autogaincontrol
        pub const AUTO_GAIN_CONTROL: &str = "autoGainControl";

        /// The number of independent channels of sound that the audio data contains,
        /// i.e. the number of audio samples per sample frame, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-channelcount
        pub const CHANNEL_COUNT: &str = "channelCount";

        /// When one or more audio streams is being played in the processes of
        /// various microphones, it is often desirable to attempt to remove
        /// all the sound being played from the input signals recorded by the microphones.
        /// This is referred to as echo cancellation, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-echocancellation
        pub const ECHO_CANCELLATION: &str = "echoCancellation";

        /// The latency or latency range, in seconds, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-latency
        pub const LATENCY: &str = "latency";

        /// Noise suppression is often desirable on the input signal recorded by the microphone,
        /// as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-noisesuppression
        pub const NOISE_SUPPRESSION: &str = "noiseSuppression";

        /// The sample rate in samples per second for the audio data, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-samplerate
        pub const SAMPLE_RATE: &str = "sampleRate";

        /// The linear sample size in bits. This constraint can only
        /// be satisfied for audio devices that produce linear samples, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-samplesize
        pub const SAMPLE_SIZE: &str = "sampleSize";
    }

    /// Names of all audio-only properties.
    pub fn names() -> Vec<&'static str> {
        use self::name::*;

        vec![
            AUTO_GAIN_CONTROL,
            CHANNEL_COUNT,
            ECHO_CANCELLATION,
            LATENCY,
            NOISE_SUPPRESSION,
            SAMPLE_RATE,
            SAMPLE_SIZE,
        ]
    }
}

/// Properties that apply only to video device types.
pub mod video_only {
    /// Names of audio-only properties.
    pub mod name {
        /// The exact aspect ratio (width in pixels divided by height in pixels,
        /// represented as a double rounded to the tenth decimal place),
        /// as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-aspectratio
        pub const ASPECT_RATIO: &str = "aspectRatio";

        /// The directions that the camera can face, as seen from the user's perspective,
        /// as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-facingmode
        pub const FACING_MODE: &str = "facingMode";

        /// The exact frame rate (frames per second) or frame rate range,
        /// as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-framerate
        pub const FRAME_RATE: &str = "frameRate";

        /// The height or height range, in pixels, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-height
        pub const HEIGHT: &str = "height";

        /// The width or width range, in pixels, as defined in the [spec][spec].
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-width
        pub const WIDTH: &str = "width";

        /// The means by which the resolution can be derived by the client, as defined in the [spec][spec].
        ///
        /// In other words, whether the client is allowed to use cropping and downscaling on the camera output.
        ///
        /// [spec]: https://www.w3.org/TR/mediacapture-streams/#dfn-resizemode
        pub const RESIZE_MODE: &str = "resizeMode";
    }

    /// Names of all video-only properties.
    pub fn names() -> Vec<&'static str> {
        use self::name::*;
        vec![
            ASPECT_RATIO,
            FACING_MODE,
            FRAME_RATE,
            HEIGHT,
            WIDTH,
            RESIZE_MODE,
        ]
    }
}

/// Names of all properties.
pub mod name {
    pub use super::audio_only::name::*;
    pub use super::common::name::*;
    pub use super::video_only::name::*;
}

/// Names of all properties.
pub fn names() -> Vec<&'static str> {
    let mut all = vec![];
    all.append(&mut self::common::names());
    all.append(&mut self::audio_only::names());
    all.append(&mut self::video_only::names());
    all
}
