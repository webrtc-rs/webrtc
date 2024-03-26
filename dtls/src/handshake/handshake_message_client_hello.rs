#[cfg(test)]
mod handshake_message_client_hello_test;

use std::fmt;
use std::io::{BufReader, BufWriter};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::handshake_random::*;
use super::*;
use crate::cipher_suite::*;
use crate::compression_methods::*;
use crate::extension::*;
use crate::record_layer::record_layer_header::*;

/*
When a client first connects to a server it is required to send
the client hello as its first message.  The client can also send a
client hello in response to a hello request or on its own
initiative in order to renegotiate the security parameters in an
existing connection.
*/
#[derive(Clone)]
pub struct HandshakeMessageClientHello {
    pub(crate) version: ProtocolVersion,
    pub(crate) random: HandshakeRandom,
    pub(crate) cookie: Vec<u8>,

    pub(crate) cipher_suites: Vec<CipherSuiteId>,
    pub(crate) compression_methods: CompressionMethods,
    pub(crate) extensions: Vec<Extension>,
}

impl PartialEq for HandshakeMessageClientHello {
    fn eq(&self, other: &Self) -> bool {
        if !(self.version == other.version
            && self.random == other.random
            && self.cookie == other.cookie
            && self.compression_methods == other.compression_methods
            && self.extensions == other.extensions
            && self.cipher_suites.len() == other.cipher_suites.len())
        {
            return false;
        }

        for i in 0..self.cipher_suites.len() {
            if self.cipher_suites[i] != other.cipher_suites[i] {
                return false;
            }
        }

        true
    }
}

impl fmt::Debug for HandshakeMessageClientHello {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut cipher_suites_str = String::new();
        for cipher_suite in &self.cipher_suites {
            cipher_suites_str += &cipher_suite.to_string();
            cipher_suites_str += " ";
        }
        let s = [
            format!("version: {:?} random: {:?}", self.version, self.random),
            format!("cookie: {:?}", self.cookie),
            format!("cipher_suites: {cipher_suites_str:?}"),
            format!("compression_methods: {:?}", self.compression_methods),
            format!("extensions: {:?}", self.extensions),
        ];
        write!(f, "{}", s.join(" "))
    }
}

const HANDSHAKE_MESSAGE_CLIENT_HELLO_VARIABLE_WIDTH_START: usize = 34;

impl HandshakeMessageClientHello {
    pub fn handshake_type(&self) -> HandshakeType {
        HandshakeType::ClientHello
    }

    pub fn size(&self) -> usize {
        let mut len = 0;

        len += 2; // version.major+minor
        len += self.random.size();

        // SessionID
        len += 1;

        len += 1 + self.cookie.len();

        len += 2 + 2 * self.cipher_suites.len();

        len += self.compression_methods.size();

        len += 2;
        for extension in &self.extensions {
            len += extension.size();
        }

        len
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        if self.cookie.len() > 255 {
            return Err(Error::ErrCookieTooLong);
        }

        writer.write_u8(self.version.major)?;
        writer.write_u8(self.version.minor)?;
        self.random.marshal(writer)?;

        // SessionID
        writer.write_u8(0x00)?;

        writer.write_u8(self.cookie.len() as u8)?;
        writer.write_all(&self.cookie)?;

        writer.write_u16::<BigEndian>(2 * self.cipher_suites.len() as u16)?;
        for cipher_suite in &self.cipher_suites {
            writer.write_u16::<BigEndian>(*cipher_suite as u16)?;
        }

        self.compression_methods.marshal(writer)?;

        let mut extension_buffer = vec![];
        {
            let mut extension_writer = BufWriter::<&mut Vec<u8>>::new(extension_buffer.as_mut());
            for extension in &self.extensions {
                extension.marshal(&mut extension_writer)?;
            }
        }

        writer.write_u16::<BigEndian>(extension_buffer.len() as u16)?;
        writer.write_all(&extension_buffer)?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let major = reader.read_u8()?;
        let minor = reader.read_u8()?;
        let random = HandshakeRandom::unmarshal(reader)?;

        // Session ID
        reader.read_u8()?;

        let cookie_len = reader.read_u8()? as usize;
        let mut cookie = vec![0; cookie_len];
        reader.read_exact(&mut cookie)?;

        let cipher_suites_len = reader.read_u16::<BigEndian>()? as usize / 2;
        let mut cipher_suites = vec![];
        for _ in 0..cipher_suites_len {
            let id: CipherSuiteId = reader.read_u16::<BigEndian>()?.into();
            //let cipher_suite = cipher_suite_for_id(id)?;
            cipher_suites.push(id);
        }

        let compression_methods = CompressionMethods::unmarshal(reader)?;
        let mut extensions = vec![];

        let extension_buffer_len = reader.read_u16::<BigEndian>()? as usize;
        let mut extension_buffer = vec![0u8; extension_buffer_len];
        reader.read_exact(&mut extension_buffer)?;

        let mut offset = 0;
        while offset < extension_buffer_len {
            let mut extension_reader = BufReader::new(&extension_buffer[offset..]);
            if let Ok(extension) = Extension::unmarshal(&mut extension_reader) {
                extensions.push(extension);
            } else {
                log::warn!(
                    "Unsupported Extension Type {} {}",
                    extension_buffer[offset],
                    extension_buffer[offset + 1]
                );
            }

            let extension_len =
                u16::from_be_bytes([extension_buffer[offset + 2], extension_buffer[offset + 3]])
                    as usize;
            offset += 4 + extension_len;
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
