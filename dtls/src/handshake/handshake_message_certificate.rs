use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::*;

#[cfg(test)]
mod handshake_message_certificate_test;

const HANDSHAKE_MESSAGE_CERTIFICATE_LENGTH_FIELD_SIZE: usize = 3;

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct HandshakeMessageCertificate {
    pub(crate) certificate: Vec<Vec<u8>>,
}

impl HandshakeMessageCertificate {
    pub fn handshake_type(&self) -> HandshakeType {
        HandshakeType::Certificate
    }

    pub fn size(&self) -> usize {
        let mut len = 3;

        for r in &self.certificate {
            len += HANDSHAKE_MESSAGE_CERTIFICATE_LENGTH_FIELD_SIZE + r.len();
        }

        len
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        let mut payload_size = 0;
        for r in &self.certificate {
            payload_size += HANDSHAKE_MESSAGE_CERTIFICATE_LENGTH_FIELD_SIZE + r.len();
        }

        // Total Payload Size
        writer.write_u24::<BigEndian>(payload_size as u32)?;

        for r in &self.certificate {
            // Certificate Length
            writer.write_u24::<BigEndian>(r.len() as u32)?;

            // Certificate body
            writer.write_all(r)?;
        }

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let mut certificate: Vec<Vec<u8>> = vec![];

        let payload_size = reader.read_u24::<BigEndian>()? as usize;
        let mut offset = 0;
        while offset < payload_size {
            let certificate_len = reader.read_u24::<BigEndian>()? as usize;
            offset += HANDSHAKE_MESSAGE_CERTIFICATE_LENGTH_FIELD_SIZE;

            let mut buf = vec![0; certificate_len];
            reader.read_exact(&mut buf)?;
            offset += certificate_len;

            certificate.push(buf);
        }

        Ok(HandshakeMessageCertificate { certificate })
    }
}
