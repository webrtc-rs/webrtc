#[cfg(test)]
mod packet_test;

use crate::{error::Error, header::*, packetizer::Marshaller};

use bytes::{BufMut, Bytes, BytesMut};
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

impl Marshaller for Packet {
    /// Unmarshal parses the passed byte slice and stores the result in the Header this method is called upon
    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error> {
        let header = Header::unmarshal(raw_packet)?;
        let payload = raw_packet.slice(header.marshal_size()..);

        Ok(Packet { header, payload })
    }

    /// MarshalSize returns the size of the packet once marshaled.
    fn marshal_size(&self) -> usize {
        self.header.marshal_size() + self.payload.len()
    }

    /// MarshalTo serializes the packet and writes to the buffer.
    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error> {
        let n = self.header.marshal_to(buf)?;
        buf.put(&*self.payload);
        Ok(n + self.payload.len())
    }
}
