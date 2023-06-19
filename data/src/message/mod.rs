#[cfg(test)]
mod message_test;

pub mod message_channel_ack;
pub mod message_channel_open;
pub mod message_type;

use bytes::{Buf, BufMut};
use message_channel_ack::*;
use message_channel_open::*;
use message_type::*;
use util::marshal::*;

use crate::error::Error;

/// A parsed DataChannel message
#[derive(Eq, PartialEq, Clone, Debug)]
pub enum Message {
    DataChannelAck(DataChannelAck),
    DataChannelOpen(DataChannelOpen),
}

impl MarshalSize for Message {
    fn marshal_size(&self) -> usize {
        match self {
            Message::DataChannelAck(m) => m.marshal_size() + MESSAGE_TYPE_LEN,
            Message::DataChannelOpen(m) => m.marshal_size() + MESSAGE_TYPE_LEN,
        }
    }
}

impl Marshal for Message {
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize, util::Error> {
        let mut bytes_written = 0;
        let n = self.message_type().marshal_to(buf)?;
        buf = &mut buf[n..];
        bytes_written += n;
        bytes_written += match self {
            Message::DataChannelAck(_) => 0,
            Message::DataChannelOpen(open) => open.marshal_to(buf)?,
        };
        Ok(bytes_written)
    }
}

impl Unmarshal for Message {
    fn unmarshal<B>(buf: &mut B) -> Result<Self, util::Error>
    where
        Self: Sized,
        B: Buf,
    {
        if buf.remaining() < MESSAGE_TYPE_LEN {
            return Err(Error::UnexpectedEndOfBuffer {
                expected: MESSAGE_TYPE_LEN,
                actual: buf.remaining(),
            }
            .into());
        }

        match MessageType::unmarshal(buf)? {
            MessageType::DataChannelAck => Ok(Self::DataChannelAck(DataChannelAck {})),
            MessageType::DataChannelOpen => {
                Ok(Self::DataChannelOpen(DataChannelOpen::unmarshal(buf)?))
            }
        }
    }
}

impl Message {
    pub fn message_type(&self) -> MessageType {
        match self {
            Self::DataChannelAck(_) => MessageType::DataChannelAck,
            Self::DataChannelOpen(_) => MessageType::DataChannelOpen,
        }
    }
}
