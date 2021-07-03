use super::*;
use crate::error::Error;

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

impl Marshal for MessageType {
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        let b = match self {
            MessageType::DataChannelAck => MESSAGE_TYPE_ACK,
            MessageType::DataChannelOpen => MESSAGE_TYPE_OPEN,
        };

        buf.put_u8(b);

        Ok(1)
    }
}

impl Unmarshal for MessageType {
    fn unmarshal<B>(buf: &mut B) -> Result<Self>
    where
        B: Buf,
    {
        let required_len = MESSAGE_TYPE_LEN;
        if buf.remaining() < required_len {
            return Err(Error::UnexpectedEndOfBuffer {
                expected: required_len,
                actual: buf.remaining(),
            }
            .into());
        }

        let b = buf.get_u8();

        match b {
            MESSAGE_TYPE_ACK => Ok(Self::DataChannelAck),
            MESSAGE_TYPE_OPEN => Ok(Self::DataChannelOpen),
            _ => Err(Error::InvalidMessageType(b).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::{Bytes, BytesMut};

    #[test]
    fn unmarshal_open_success() -> Result<()> {
        let mut bytes = Bytes::from_static(&[0x03]);
        let msg_type = MessageType::unmarshal(&mut bytes)?;

        assert_eq!(msg_type, MessageType::DataChannelOpen);

        Ok(())
    }

    #[test]
    fn unmarshal_ack_success() -> Result<()> {
        let mut bytes = Bytes::from_static(&[0x02]);
        let msg_type = MessageType::unmarshal(&mut bytes)?;

        assert_eq!(msg_type, MessageType::DataChannelAck);
        Ok(())
    }

    #[test]
    fn unmarshal_invalid_message_type() -> Result<()> {
        let mut bytes = Bytes::from_static(&[0x01]);
        match MessageType::unmarshal(&mut bytes) {
            Ok(_) => assert!(false, "expected Error, but got Ok"),
            Err(err) => {
                if let Some(err) = err.downcast_ref::<Error>() {
                    match err {
                        &Error::InvalidMessageType(0x01) => return Ok(()),
                        _ => {}
                    };
                }
                assert!(
                    false,
                    "unexpected err {:?}, want {:?}",
                    err,
                    Error::InvalidMessageType(0x01)
                );
            }
        }
        Ok(())
    }

    #[test]
    fn marshal_size() -> Result<()> {
        let ack = MessageType::DataChannelAck;
        let marshal_size = ack.marshal_size();

        assert_eq!(marshal_size, MESSAGE_TYPE_LEN);
        Ok(())
    }

    #[test]
    fn marshal() -> Result<()> {
        let mut buf = BytesMut::with_capacity(MESSAGE_TYPE_LEN);
        buf.resize(MESSAGE_TYPE_LEN, 0u8);
        let msg_type = MessageType::DataChannelAck;
        let n = msg_type.marshal_to(&mut buf)?;
        let bytes = buf.freeze();

        assert_eq!(n, MESSAGE_TYPE_LEN);
        assert_eq!(&bytes[..], &[0x02]);
        Ok(())
    }
}
