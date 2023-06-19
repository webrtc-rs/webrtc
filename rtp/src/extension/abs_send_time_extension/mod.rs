#[cfg(test)]
mod abs_send_time_extension_test;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::{Buf, BufMut};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;

pub const ABS_SEND_TIME_EXTENSION_SIZE: usize = 3;

/// AbsSendTimeExtension is a extension payload format in
/// http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
#[derive(PartialEq, Eq, Debug, Default, Copy, Clone)]
pub struct AbsSendTimeExtension {
    pub timestamp: u64,
}

impl Unmarshal for AbsSendTimeExtension {
    /// Unmarshal parses the passed byte slice and stores the result in the members.
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self, util::Error>
    where
        Self: Sized,
        B: Buf,
    {
        if raw_packet.remaining() < ABS_SEND_TIME_EXTENSION_SIZE {
            return Err(Error::ErrBufferTooSmall.into());
        }

        let b0 = raw_packet.get_u8();
        let b1 = raw_packet.get_u8();
        let b2 = raw_packet.get_u8();
        let timestamp = (b0 as u64) << 16 | (b1 as u64) << 8 | b2 as u64;

        Ok(AbsSendTimeExtension { timestamp })
    }
}

impl MarshalSize for AbsSendTimeExtension {
    /// MarshalSize returns the size of the AbsSendTimeExtension once marshaled.
    fn marshal_size(&self) -> usize {
        ABS_SEND_TIME_EXTENSION_SIZE
    }
}

impl Marshal for AbsSendTimeExtension {
    /// MarshalTo serializes the members to buffer.
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize, util::Error> {
        if buf.remaining_mut() < ABS_SEND_TIME_EXTENSION_SIZE {
            return Err(Error::ErrBufferTooSmall.into());
        }

        buf.put_u8(((self.timestamp & 0xFF0000) >> 16) as u8);
        buf.put_u8(((self.timestamp & 0xFF00) >> 8) as u8);
        buf.put_u8((self.timestamp & 0xFF) as u8);

        Ok(ABS_SEND_TIME_EXTENSION_SIZE)
    }
}

impl AbsSendTimeExtension {
    /// Estimate absolute send time according to the receive time.
    /// Note that if the transmission delay is larger than 64 seconds, estimated time will be wrong.
    pub fn estimate(&self, receive: SystemTime) -> SystemTime {
        let receive_ntp = unix2ntp(receive);
        let mut ntp = receive_ntp & 0xFFFFFFC000000000 | (self.timestamp & 0xFFFFFF) << 14;
        if receive_ntp < ntp {
            // Receive time must be always later than send time
            ntp -= 0x1000000 << 14;
        }

        ntp2unix(ntp)
    }

    /// NewAbsSendTimeExtension makes new AbsSendTimeExtension from time.Time.
    pub fn new(send_time: SystemTime) -> Self {
        AbsSendTimeExtension {
            timestamp: unix2ntp(send_time) >> 14,
        }
    }
}

pub fn unix2ntp(st: SystemTime) -> u64 {
    let u = st
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_nanos() as u64;
    let mut s = u / 1_000_000_000;
    s += 0x83AA7E80; //offset in seconds between unix epoch and ntp epoch
    let mut f = u % 1_000_000_000;
    f <<= 32;
    f /= 1_000_000_000;
    s <<= 32;

    s | f
}

pub fn ntp2unix(t: u64) -> SystemTime {
    let mut s = t >> 32;
    let mut f = t & 0xFFFFFFFF;
    f *= 1_000_000_000;
    f >>= 32;
    s -= 0x83AA7E80;
    let u = s * 1_000_000_000 + f;

    UNIX_EPOCH
        .checked_add(Duration::new(u / 1_000_000_000, (u % 1_000_000_000) as u32))
        .unwrap_or(UNIX_EPOCH)
}
