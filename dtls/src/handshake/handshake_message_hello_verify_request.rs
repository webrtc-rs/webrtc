#[cfg(test)]
mod handshake_message_hello_verify_request_test;

use std::io::{Read, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};

use super::*;
use crate::record_layer::record_layer_header::*;

/*
   The definition of HelloVerifyRequest is as follows:

   struct {
     ProtocolVersion server_version;
     opaque cookie<0..2^8-1>;
   } HelloVerifyRequest;

   The HelloVerifyRequest message type is hello_verify_request(3).

   When the client sends its ClientHello message to the server, the server
   MAY respond with a HelloVerifyRequest message.  This message contains
   a stateless cookie generated using the technique of [PHOTURIS].  The
   client MUST retransmit the ClientHello with the cookie added.

   https://tools.ietf.org/html/rfc6347#section-4.2.1
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeMessageHelloVerifyRequest {
    pub(crate) version: ProtocolVersion,
    pub(crate) cookie: Vec<u8>,
}

impl HandshakeMessageHelloVerifyRequest {
    pub fn handshake_type(&self) -> HandshakeType {
        HandshakeType::HelloVerifyRequest
    }

    pub fn size(&self) -> usize {
        1 + 1 + 1 + self.cookie.len()
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        if self.cookie.len() > 255 {
            return Err(Error::ErrCookieTooLong);
        }

        writer.write_u8(self.version.major)?;
        writer.write_u8(self.version.minor)?;
        writer.write_u8(self.cookie.len() as u8)?;
        writer.write_all(&self.cookie)?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let major = reader.read_u8()?;
        let minor = reader.read_u8()?;
        let cookie_length = reader.read_u8()?;
        let mut cookie = vec![];
        reader.read_to_end(&mut cookie)?;

        if cookie.len() < cookie_length as usize {
            return Err(Error::ErrBufferTooSmall);
        }

        Ok(HandshakeMessageHelloVerifyRequest {
            version: ProtocolVersion { major, minor },
            cookie,
        })
    }
}
