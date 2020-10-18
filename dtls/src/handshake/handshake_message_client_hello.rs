#[cfg(test)]
mod handshake_message_client_hello_test;

use super::handshake_random::*;
use super::*;
use crate::cipher_suite::*;
use crate::compression_methods::*;
use crate::extension::*;
use crate::record_layer::record_layer_header::*;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

/*
When a client first connects to a server it is required to send
the client hello as its first message.  The client can also send a
client hello in response to a hello request or on its own
initiative in order to renegotiate the security parameters in an
existing connection.
*/
pub struct HandshakeMessageClientHello {
    version: ProtocolVersion,
    random: HandshakeRandom,
    cookie: Vec<u8>,

    cipher_suites: Vec<Box<dyn CipherSuite>>,
    compression_methods: CompressionMethods,
    extensions: Vec<Extension>,
}

const HANDSHAKE_MESSAGE_CLIENT_HELLO_VARIABLE_WIDTH_START: usize = 34;

impl HandshakeMessageClientHello {
    fn handshake_type() -> HandshakeType {
        HandshakeType::ClientHello
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        if self.cookie.len() > 255 {
            return Err(ERR_COOKIE_TOO_LONG.clone());
        }

        writer.write_u8(self.version.major)?;
        writer.write_u8(self.version.minor)?;
        self.random.marshal(writer)?;

        // SessionID
        writer.write_u8(0x00)?;

        writer.write_u8(self.cookie.len() as u8)?;
        writer.write_all(&self.cookie)?;

        writer.write_u16::<BigEndian>(self.cipher_suites.len() as u16)?;
        for _cipher_suite in &self.cipher_suites {
            //TODO: cipher_suite.marshal(writer)?;
        }

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

        let cookie_len = reader.read_u8()? as usize;
        let mut cookie = vec![0; cookie_len];
        reader.read_exact(&mut cookie)?;

        let cipher_suites_len = reader.read_u16::<BigEndian>()? as usize;
        let cipher_suites = vec![];
        for _ in 0..cipher_suites_len {
            //TODO: let cipher_suite = CipherSuite::unmarshal(reader)?;
            // cipher_suites.push(cipher_suite);
        }

        let compression_methods = CompressionMethods::unmarshal(reader)?;

        let extensions_len = reader.read_u16::<BigEndian>()? as usize;
        let mut extensions = vec![];
        for _ in 0..extensions_len {
            let extension = Extension::unmarshal(reader)?;
            extensions.push(extension);
        }

        Ok(HandshakeMessageClientHello {
            version: ProtocolVersion { major, minor },
            random,
            cookie,

            cipher_suites,
            compression_methods,
            extensions,
        })
    }
}
