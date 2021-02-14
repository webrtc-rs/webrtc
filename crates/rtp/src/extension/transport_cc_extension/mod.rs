use crate::errors::ExtensionError;
use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;

mod transport_cc_extension_test;

// transport-wide sequence
const TRANSPORT_CC_EXTENSION_SIZE: usize = 2;

// TransportCCExtension is a extension payload format in
// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01
// 0                   1                   2                   3
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |       0xBE    |    0xDE       |           length=1            |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |  ID   | L=1   |transport-wide sequence number | zero padding  |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(PartialEq, Debug, Default)]
pub struct TransportCCExtension {
    pub transport_sequence: u16,
}

impl TransportCCExtension {
    // Marshal serializes the members to buffer
    pub fn marshal(&self) -> Result<BytesMut, ExtensionError> {
        let mut buf = vec![0u8; TRANSPORT_CC_EXTENSION_SIZE];
        BigEndian::write_u16(&mut buf[0..2], self.transport_sequence);
        Ok(BytesMut::from(buf.as_slice()))
    }

    // Unmarshal parses the passed byte slice and stores the result in the members
    pub fn unmarshal(&mut self, raw_data: &mut BytesMut) -> Result<(), ExtensionError> {
        if raw_data.len() < TRANSPORT_CC_EXTENSION_SIZE {
            return Err(ExtensionError::TooSmall);
        }

        self.transport_sequence = BigEndian::read_u16(&raw_data[0..2]);
        Ok(())
    }
}
