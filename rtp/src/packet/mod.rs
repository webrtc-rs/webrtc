use crate::header::*;
use std::fmt;
use util::Error;

mod packet_test;

// Packet represents an RTP Packet
// NOTE: Raw is populated by Marshal/Unmarshal and should not be modified
#[derive(Debug, Eq, PartialEq, Default)]
pub struct Packet {
    pub header: Header,
    pub payload: Vec<u8>,
    pub raw: Vec<u8>,
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

    // Unmarshal parses the passed byte slice and stores the result in the Header this method is called upon
    pub fn unmarshal(&mut self, raw_packet: &mut [u8]) -> Result<(), Error> {
        self.header.unmarshal(raw_packet)?;

        self.payload = raw_packet[self.header.payload_offset..].to_vec();
        self.raw = raw_packet.to_vec();

        Ok(())
    }

    // MarshalSize returns the size of the packet once marshaled.
    pub fn marshal_size(&self) -> usize {
        self.header.marshal_size() + self.payload.len()
    }

    // Marshal serializes the header and writes to the buffer.
    pub fn marshal(&mut self) -> Result<Vec<u8>, Error> {
        let mut buf = vec![0u8; self.marshal_size()];

        let size = self.marshal_to(&mut buf)?;

        Ok(writer.flush()?)
    }
}
