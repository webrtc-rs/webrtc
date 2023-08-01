#[cfg(test)]
mod handshake_message_server_hello_done_test;

use std::io::{Read, Write};

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeMessageServerHelloDone;

impl HandshakeMessageServerHelloDone {
    pub fn handshake_type(&self) -> HandshakeType {
        HandshakeType::ServerHelloDone
    }

    pub fn size(&self) -> usize {
        0
    }

    pub fn marshal<W: Write>(&self, _writer: &mut W) -> Result<()> {
        Ok(())
    }

    pub fn unmarshal<R: Read>(_reader: &mut R) -> Result<Self> {
        Ok(HandshakeMessageServerHelloDone {})
    }
}
