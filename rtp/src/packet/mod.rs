#[cfg(test)]
mod packet_test;

use std::fmt;

use bytes::{Buf, BufMut, Bytes};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;
use crate::header::*;

/// Packet represents an RTP Packet
/// NOTE: Raw is populated by Marshal/Unmarshal and should not be modified
#[derive(Debug, Eq, PartialEq, Default, Clone)]
pub struct Packet {
    pub header: Header,
    pub payload: Bytes,
}

impl fmt::Display for Packet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = "RTP PACKET:\n".to_string();

        out += format!("\tVersion: {}\n", self.header.version).as_str();
        out += format!("\tMarker: {}\n", self.header.marker).as_str();
        out += format!("\tPayload Type: {}\n", self.header.payload_type).as_str();
        out += format!("\tSequence Number: {}\n", self.header.sequence_number).as_str();
        out += format!("\tTimestamp: {}\n", self.header.timestamp).as_str();
        out += format!("\tSSRC: {} ({:x})\n", self.header.ssrc, self.header.ssrc).as_str();
        out += format!("\tPayload Length: {}\n", self.payload.len()).as_str();

        write!(f, "{out}")
    }
}

impl Unmarshal for Packet {
    /// Unmarshal parses the passed byte slice and stores the result in the Header this method is called upon
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self, util::Error>
    where
        Self: Sized,
        B: Buf,
    {
        let header = Header::unmarshal(raw_packet)?;
        let payload_len = raw_packet.remaining();
        let payload = raw_packet.copy_to_bytes(payload_len);
        if header.padding {
            if payload_len > 0 {
                let padding_len = payload[payload_len - 1] as usize;
                if padding_len <= payload_len {
                    Ok(Packet {
                        header,
                        payload: payload.slice(..payload_len - padding_len),
                    })
                } else {
                    Err(Error::ErrShortPacket.into())
                }
            } else {
                Err(Error::ErrShortPacket.into())
            }
        } else {
            Ok(Packet { header, payload })
        }
    }
}

impl MarshalSize for Packet {
    /// MarshalSize returns the size of the packet once marshaled.
    fn marshal_size(&self) -> usize {
        let payload_len = self.payload.len();
        let padding_len = if self.header.padding {
            let padding_len = get_padding(payload_len);
            if padding_len == 0 {
                4
            } else {
                padding_len
            }
        } else {
            0
        };
        self.header.marshal_size() + payload_len + padding_len
    }
}

impl Marshal for Packet {
    /// MarshalTo serializes the packet and writes to the buffer.
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize, util::Error> {
        if buf.remaining_mut() < self.marshal_size() {
            return Err(Error::ErrBufferTooSmall.into());
        }

        let n = self.header.marshal_to(buf)?;
        buf = &mut buf[n..];
        buf.put(&*self.payload);
        let padding_len = if self.header.padding {
            let mut padding_len = get_padding(self.payload.len());
            if padding_len == 0 {
                padding_len = 4;
            }
            for i in 0..padding_len {
                if i != padding_len - 1 {
                    buf.put_u8(0);
                } else {
                    buf.put_u8(padding_len as u8);
                }
            }
            padding_len
        } else {
            0
        };

        Ok(n + self.payload.len() + padding_len)
    }
}

/// getPadding Returns the padding required to make the length a multiple of 4
fn get_padding(len: usize) -> usize {
    if len % 4 == 0 {
        0
    } else {
        4 - (len % 4)
    }
}
