#[cfg(test)]
mod packet_test;

use crate::{error::Error, header::*};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use anyhow::Result;
use bytes::{Buf, BufMut, Bytes};
use std::fmt;

/// Packet represents an RTP Packet
/// NOTE: Raw is populated by Marshal/Unmarshal and should not be modified
#[derive(Debug, Eq, PartialEq, Default)]
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

        write!(f, "{}", out)
    }
}

impl Unmarshal for Packet {
    /// Unmarshal parses the passed byte slice and stores the result in the Header this method is called upon
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        let header = Header::unmarshal(raw_packet)?;
        let payload = raw_packet.copy_to_bytes(raw_packet.remaining());
        if header.padding && !payload.is_empty() {
            let payload_len = payload.len();
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
            Ok(Packet { header, payload })
        }
    }
}

impl MarshalSize for Packet {
    /// MarshalSize returns the size of the packet once marshaled.
    fn marshal_size(&self) -> usize {
        let payload_len = self.payload.len();
        let padding_len = if self.header.padding {
            get_padding(payload_len)
        } else {
            0
        };
        self.header.marshal_size() + payload_len + padding_len
    }
}

impl Marshal for Packet {
    /// MarshalTo serializes the packet and writes to the buffer.
    fn marshal_to<B>(&self, buf: &mut B) -> Result<usize>
    where
        B: BufMut,
    {
        let n = self.header.marshal_to(buf)?;
        buf.put(&*self.payload);
        let padding_len = if self.header.padding {
            let padding_len = get_padding(self.payload.len());
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
