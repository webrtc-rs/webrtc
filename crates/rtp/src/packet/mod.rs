use crate::errors::RTPError;
use crate::header::*;
use bytes::BytesMut;
use std::fmt;

mod packet_test;

// Packet represents an RTP Packet
// NOTE: Raw is populated by Marshal/Unmarshal and should not be modified
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct Packet {
    pub header: Header,
    pub payload: BytesMut,
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

impl Packet {
    pub fn new() -> Self {
        Packet::default()
    }

    /// Unmarshal parses the passed byte slice and stores the result in the Header this method is called upon
    pub fn unmarshal(&mut self, buf: &mut BytesMut) -> Result<(), RTPError> {
        let size = self.header.unmarshal(buf)?;

        self.payload = buf[size..].into();

        Ok(())
    }

    /// MarshalSize returns the size of the packet once marshaled.
    pub fn marshal_size(&mut self) -> usize {
        self.header.marshal_size() + self.payload.len()
    }

    pub fn marshal_to(&mut self, buf: &mut BytesMut) -> Result<usize, RTPError> {
        let size = self.header.marshal_to(buf)?;

        // Make sure the buffer is large enough to hold the packet.
        if size + self.payload.len() > buf.len() {
            return Err(RTPError::ShortBuffer);
        }

        buf[size..size + self.payload.len()].copy_from_slice(&self.payload);

        Ok(size + self.payload.len())
    }

    /// Marshal serializes the packet into bytes.
    pub fn marshal(&mut self) -> Result<BytesMut, RTPError> {
        let mut buf = BytesMut::new();
        buf.resize(self.marshal_size(), 0u8);

        let size = self.marshal_to(&mut buf)?;

        buf.truncate(size);

        Ok(buf)
    }
}
