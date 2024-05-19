use std::borrow::Cow;
use std::fmt;

use util::{Marshal, MarshalSize};

pub mod abs_send_time_extension;
pub mod audio_level_extension;
pub mod playout_delay_extension;
pub mod transport_cc_extension;
pub mod video_orientation_extension;

/// A generic RTP header extension.
pub enum HeaderExtension {
    AbsSendTime(abs_send_time_extension::AbsSendTimeExtension),
    AudioLevel(audio_level_extension::AudioLevelExtension),
    PlayoutDelay(playout_delay_extension::PlayoutDelayExtension),
    TransportCc(transport_cc_extension::TransportCcExtension),
    VideoOrientation(video_orientation_extension::VideoOrientationExtension),

    /// A custom extension
    Custom {
        uri: Cow<'static, str>,
        extension: Box<dyn Marshal + Send + Sync + 'static>,
    },
}

impl HeaderExtension {
    pub fn uri(&self) -> Cow<'static, str> {
        use HeaderExtension::*;

        match self {
            AbsSendTime(_) => "http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time".into(),
            AudioLevel(_) => "urn:ietf:params:rtp-hdrext:ssrc-audio-level".into(),
            PlayoutDelay(_) => "http://www.webrtc.org/experiments/rtp-hdrext/playout-delay".into(),
            TransportCc(_) => {
                "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01".into()
            }
            VideoOrientation(_) => "urn:3gpp:video-orientation".into(),
            Custom { uri, .. } => uri.clone(),
        }
    }

    pub fn is_same(&self, other: &Self) -> bool {
        use HeaderExtension::*;
        match (self, other) {
            (AbsSendTime(_), AbsSendTime(_)) => true,
            (AudioLevel(_), AudioLevel(_)) => true,
            (TransportCc(_), TransportCc(_)) => true,
            (VideoOrientation(_), VideoOrientation(_)) => true,
            (Custom { uri, .. }, Custom { uri: other_uri, .. }) => uri == other_uri,
            _ => false,
        }
    }
}

impl MarshalSize for HeaderExtension {
    fn marshal_size(&self) -> usize {
        use HeaderExtension::*;
        match self {
            AbsSendTime(ext) => ext.marshal_size(),
            AudioLevel(ext) => ext.marshal_size(),
            PlayoutDelay(ext) => ext.marshal_size(),
            TransportCc(ext) => ext.marshal_size(),
            VideoOrientation(ext) => ext.marshal_size(),
            Custom { extension: ext, .. } => ext.marshal_size(),
        }
    }
}

impl Marshal for HeaderExtension {
    fn marshal_to(&self, buf: &mut [u8]) -> util::Result<usize> {
        use HeaderExtension::*;
        match self {
            AbsSendTime(ext) => ext.marshal_to(buf),
            AudioLevel(ext) => ext.marshal_to(buf),
            PlayoutDelay(ext) => ext.marshal_to(buf),
            TransportCc(ext) => ext.marshal_to(buf),
            VideoOrientation(ext) => ext.marshal_to(buf),
            Custom { extension: ext, .. } => ext.marshal_to(buf),
        }
    }
}

impl fmt::Debug for HeaderExtension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use HeaderExtension::*;

        match self {
            AbsSendTime(ext) => f.debug_tuple("AbsSendTime").field(ext).finish(),
            AudioLevel(ext) => f.debug_tuple("AudioLevel").field(ext).finish(),
            PlayoutDelay(ext) => f.debug_tuple("PlayoutDelay").field(ext).finish(),
            TransportCc(ext) => f.debug_tuple("TransportCc").field(ext).finish(),
            VideoOrientation(ext) => f.debug_tuple("VideoOrientation").field(ext).finish(),
            Custom { uri, extension: _ } => f.debug_struct("Custom").field("uri", uri).finish(),
        }
    }
}
