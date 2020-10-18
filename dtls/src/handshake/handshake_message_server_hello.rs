#[cfg(test)]
mod handshake_message_server_hello_test;

use super::handshake_random::*;
use super::*;
use crate::cipher_suite::*;
use crate::compression_methods::*;
use crate::extension::*;
use crate::record_layer::record_layer_header::*;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

/*
The server will send this message in response to a ClientHello
message when it was able to find an acceptable set of algorithms.
If it cannot find such a match, it will respond with a handshake
failure alert.
https://tools.ietf.org/html/rfc5246#section-7.4.1.3
*/
pub struct HandshakeMessageServerHello {
    version: ProtocolVersion,
    random: HandshakeRandom,

    cipher_suite: Box<dyn CipherSuite>,
    compression_methods: CompressionMethods,
    extensions: Vec<Extension>,
}

impl HandshakeMessageServerHello {
    fn handshake_type() -> HandshakeType {
        HandshakeType::ServerHello
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_u8(self.version.major)?;
        writer.write_u8(self.version.minor)?;
        self.random.marshal(writer)?;

        // SessionID
        writer.write_u8(0x00)?;

        writer.write_u16::<BigEndian>(self.cipher_suite.id() as u16)?;

        self.compression_methods.marshal(writer)?;

        writer.write_u16::<BigEndian>(self.extensions.len() as u16)?;
        for extension in &self.extensions {
            extension.marshal(writer)?;
        }

        Ok(())
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let major = reader.read_u8()?;
        let minor = reader.read_u8()?;
        let random = HandshakeRandom::unmarshal(reader)?;

        // Session ID
        reader.read_u8()?;

        let id: CipherSuiteID = reader.read_u16::<BigEndian>()?.into();
        let cipher_suite = cipher_suite_for_id(id)?;

        let compression_methods = CompressionMethods::unmarshal(reader)?;

        let extensions_len = reader.read_u16::<BigEndian>()? as usize;
        let mut extensions = vec![];
        for _ in 0..extensions_len {
            let extension = Extension::unmarshal(reader)?;
            extensions.push(extension);
        }

        Ok(HandshakeMessageServerHello {
            version: ProtocolVersion { major, minor },
            random,

            cipher_suite,
            compression_methods,
            extensions,
        })
    }
}
