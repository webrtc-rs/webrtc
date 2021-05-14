use bytes::{Buf, BufMut};

use crate::{
    error::MessageTypeError,
    marshal::{Marshal, MarshalSize, Unmarshal},
};

// The first byte in a `Message` that specifies its type:
const MESSAGE_TYPE_ACK: u8 = 0x02;
const MESSAGE_TYPE_OPEN: u8 = 0x03;

const MESSAGE_TYPE_LEN: usize = 1;

// A parsed DataChannel message
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum MessageType {
    DataChannelAck,
    DataChannelOpen,
}

impl MarshalSize for MessageType {
    fn marshal_size(&self) -> usize {
        MESSAGE_TYPE_LEN
    }
}

impl Unmarshal for MessageType {
    type Error = MessageTypeError;

    fn unmarshal_from<B>(buf: &mut B) -> Result<Self, Self::Error>
    where
        B: Buf,
    {
        let required_len = MESSAGE_TYPE_LEN;
        if buf.remaining() < required_len {
            return Err(Self::Error::UnexpectedEndOfBuffer {
                expected: required_len,
                actual: buf.remaining(),
            });
        }

        let byte = buf.get_u8();

        match byte {
            MESSAGE_TYPE_ACK => Ok(Self::DataChannelAck),
            MESSAGE_TYPE_OPEN => Ok(Self::DataChannelOpen),
            _ => Err(Self::Error::InvalidMessageType { invalid_type: byte }),
        }
    }
}

impl Marshal for MessageType {
    type Error = MessageTypeError;

    fn marshal_to<B>(&self, buf: &mut B) -> Result<usize, Self::Error>
    where
        B: BufMut,
    {
        let byte = match self {
            MessageType::DataChannelAck => MESSAGE_TYPE_ACK,
            MessageType::DataChannelOpen => MESSAGE_TYPE_OPEN,
        };

        buf.put_u8(byte);

        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};

    use super::*;

    #[test]
    fn unmarshal_open_success() {
        let mut bytes = Bytes::from_static(&[0x03]);
        let msg_type = MessageType::unmarshal_from(&mut bytes).unwrap();

        assert_eq!(msg_type, MessageType::DataChannelOpen);
    }

    #[test]
    fn unmarshal_ack_success() {
        let mut bytes = Bytes::from_static(&[0x02]);
        let msg_type = MessageType::unmarshal_from(&mut bytes).unwrap();

        assert_eq!(msg_type, MessageType::DataChannelAck);
    }

    #[test]
    fn unmarshal_invalid_message_type() {
        let mut bytes = Bytes::from_static(&[0x01]);
        let result = MessageType::unmarshal_from(&mut bytes);

        assert_eq!(
            result,
            Err(MessageTypeError::InvalidMessageType { invalid_type: 0x01 })
        );
    }

    #[test]
    fn marshal_size() {
        let ack = MessageType::DataChannelAck;
        let marshal_size = ack.marshal_size();

        assert_eq!(marshal_size, MESSAGE_TYPE_LEN);
    }

    #[test]
    fn marshal() {
        let mut buf = BytesMut::with_capacity(MESSAGE_TYPE_LEN);
        let msg_type = MessageType::DataChannelAck;
        let result = msg_type.marshal_to(&mut buf);
        let bytes = buf.freeze();

        assert_eq!(result, Ok(MESSAGE_TYPE_LEN));
        assert_eq!(&bytes[..], &[0x02]);
    }
}
