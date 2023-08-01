#[cfg(test)]
mod video_orientation_extension_test;

use std::convert::{TryFrom, TryInto};

use bytes::BufMut;
use serde::{Deserialize, Serialize};
use util::marshal::Unmarshal;
use util::{Marshal, MarshalSize};

use crate::Error;

// One byte header size
pub const VIDEO_ORIENTATION_EXTENSION_SIZE: usize = 1;

/// Coordination of Video Orientation in RTP streams.
///
/// Coordination of Video Orientation consists in signaling of the current
/// orientation of the image captured on the sender side to the receiver for
/// appropriate rendering and displaying.
///
/// C = Camera: indicates the direction of the camera used for this video
///     stream. It can be used by the MTSI client in receiver to e.g. display
///     the received video differently depending on the source camera.
///
/// 0: Front-facing camera, facing the user. If camera direction is
///    unknown by the sending MTSI client in the terminal then this is the
///    default value used.
/// 1: Back-facing camera, facing away from the user.
///
/// F = Flip: indicates a horizontal (left-right flip) mirror operation on
///     the video as sent on the link.
///
///    0                   1
///    0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///   |  ID   | len=0 |0 0 0 0 C F R R|
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(PartialEq, Eq, Debug, Default, Copy, Clone, Serialize, Deserialize)]
pub struct VideoOrientationExtension {
    pub direction: CameraDirection,
    pub flip: bool,
    pub rotation: VideoRotation,
}

#[derive(Default, PartialEq, Eq, Debug, Copy, Clone, Serialize, Deserialize)]
pub enum CameraDirection {
    #[default]
    Front = 0,
    Back = 1,
}

#[derive(Default, PartialEq, Eq, Debug, Copy, Clone, Serialize, Deserialize)]
pub enum VideoRotation {
    #[default]
    Degree0 = 0,
    Degree90 = 1,
    Degree180 = 2,
    Degree270 = 3,
}

impl MarshalSize for VideoOrientationExtension {
    fn marshal_size(&self) -> usize {
        VIDEO_ORIENTATION_EXTENSION_SIZE
    }
}

impl Unmarshal for VideoOrientationExtension {
    fn unmarshal<B>(buf: &mut B) -> util::Result<Self>
    where
        Self: Sized,
        B: bytes::Buf,
    {
        if buf.remaining() < VIDEO_ORIENTATION_EXTENSION_SIZE {
            return Err(Error::ErrBufferTooSmall.into());
        }

        let b = buf.get_u8();

        let c = (b & 0b1000) >> 3;
        let f = b & 0b0100;
        let r = b & 0b0011;

        Ok(VideoOrientationExtension {
            direction: c.try_into()?,
            flip: f > 0,
            rotation: r.try_into()?,
        })
    }
}

impl Marshal for VideoOrientationExtension {
    fn marshal_to(&self, mut buf: &mut [u8]) -> util::Result<usize> {
        let c = (self.direction as u8) << 3;
        let f = if self.flip { 0b0100 } else { 0 };
        let r = self.rotation as u8;

        buf.put_u8(c | f | r);

        Ok(VIDEO_ORIENTATION_EXTENSION_SIZE)
    }
}

impl TryFrom<u8> for CameraDirection {
    type Error = util::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(CameraDirection::Front),
            1 => Ok(CameraDirection::Back),
            _ => Err(util::Error::Other(format!(
                "Unhandled camera direction: {value}"
            ))),
        }
    }
}

impl TryFrom<u8> for VideoRotation {
    type Error = util::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(VideoRotation::Degree0),
            1 => Ok(VideoRotation::Degree90),
            2 => Ok(VideoRotation::Degree180),
            3 => Ok(VideoRotation::Degree270),
            _ => Err(util::Error::Other(format!(
                "Unhandled video rotation: {value}"
            ))),
        }
    }
}
