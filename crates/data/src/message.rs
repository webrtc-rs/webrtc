use bytes::{Buf, BufMut};

use crate::marshal::{Marshal, MarshalSize, Unmarshal};

mod data_channel_ack;
mod data_channel_open;
mod message_type;

pub use data_channel_ack::{DataChannelAck, Error as DataChannelAckError};
pub use data_channel_open::{DataChannelOpen, Error as DataChannelOpenError};
pub use message_type::{Error as MessageTypeError, MessageType};

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum Error {
    // Marshal buffer was too short
    UnexpectedEndOfBuffer { expected: usize, actual: usize },

    // Declared length and actual length don't match
    ExpectedAndActualLengthMismatch { expected: usize, actual: usize },

    // DataChannel messages with a Payload Protocol Identifier we don't know how to handle
    InvalidPayloadProtocolIdentifier,

    // DataChannel message has a type we don't support
    MessageType(MessageTypeError),

    // Invalid DATA_CHANNEL_OPEN message body
    DataChannelOpen(DataChannelOpenError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEndOfBuffer { expected, actual } => {
                writeln!(
                    f,
                    "Marshal buffer was too short: (expected: {:?}, actual: {:?})",
                    expected, actual
                )
            }
            Self::InvalidPayloadProtocolIdentifier => writeln!(
                f,
                "DataChannel message payload protocol identifier is value we can't handle"
            ),
            Self::ExpectedAndActualLengthMismatch { expected, actual } => {
                writeln!(
                    f,
                    "Expected and actual length do not match: (expected: {:?}, actual: {:?})",
                    expected, actual
                )
            }
            Self::MessageType(error) => error.fmt(f),
            Self::DataChannelOpen(error) => error.fmt(f),
        }
    }
}

impl From<DataChannelOpenError> for Error {
    fn from(error: DataChannelOpenError) -> Self {
        Self::DataChannelOpen(error)
    }
}

impl From<MessageTypeError> for Error {
    fn from(error: MessageTypeError) -> Self {
        Self::MessageType(error)
    }
}

// A parsed DataChannel message
#[derive(Eq, PartialEq, Clone, Debug)]
pub enum Message {
    DataChannelAck,
    DataChannelOpen(DataChannelOpen),
}

impl MarshalSize for Message {
    fn marshal_size(&self) -> usize {
        let type_size = self.message_type().marshal_size();

        let data_size = match self {
            Message::DataChannelAck => 0,
            Message::DataChannelOpen(info) => info.marshal_size(),
        };

        type_size + data_size
    }
}

impl Unmarshal for Message {
    type Error = Error;

    fn unmarshal_from<B>(buf: &mut B) -> Result<Self, Error>
    where
        B: Buf,
    {
        match MessageType::unmarshal_from(buf)? {
            MessageType::DataChannelAck => Ok(Self::DataChannelAck),
            MessageType::DataChannelOpen => {
                let info = DataChannelOpen::unmarshal_from(buf)?;
                Ok(Self::DataChannelOpen(info))
            }
        }
    }
}

impl Marshal for Message {
    type Error = Error;

    fn marshal_to<B>(&self, buf: &mut B) -> Result<usize, Error>
    where
        B: BufMut,
    {
        let mut bytes_written = 0;
        bytes_written += self.message_type().marshal_to(buf)?;
        bytes_written += match self {
            Message::DataChannelAck => 0,
            Message::DataChannelOpen(open) => open.marshal_to(buf)?,
        };
        Ok(bytes_written)
    }
}

impl Message {
    #[inline]
    pub fn message_type(&self) -> MessageType {
        match self {
            Self::DataChannelAck => MessageType::DataChannelAck,
            Self::DataChannelOpen(_) => MessageType::DataChannelOpen,
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};

    use crate::channel_type::ChannelType;

    use super::*;

    #[test]
    fn unmarshal_open_success() {
        let mut bytes = Bytes::from_static(&[
            0x03, // message type
            0x00, // channel type
            0x0f, 0x35, // priority
            0x00, 0xff, 0x0f, 0x35, // reliability parameter
            0x00, 0x05, // label length
            0x00, 0x08, // protocol length
            0x6c, 0x61, 0x62, 0x65, 0x6c, // label
            0x70, 0x72, 0x6f, 0x74, 0x6f, 0x63, 0x6f, 0x6c, // protocol
        ]);

        let actual = Message::unmarshal_from(&mut bytes).unwrap();

        let expected = Message::DataChannelOpen(DataChannelOpen {
            channel_type: ChannelType::Reliable,
            priority: 3893,
            reliability_parameter: 16715573,
            label: b"label".iter().cloned().collect(),
            protocol: b"protocol".iter().cloned().collect(),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn unmarshal_ack_success() {
        let mut bytes = Bytes::from_static(&[0x02]);

        let actual = Message::unmarshal_from(&mut bytes);
        let expected = Ok(Message::DataChannelAck);

        assert_eq!(actual, expected);
    }

    #[test]
    fn unmarshal_invalid_message_type() {
        let mut bytes = Bytes::from_static(&[0x01]);

        let actual = Message::unmarshal_from(&mut bytes);
        let expected = Err(Error::MessageType(MessageTypeError::InvalidMessageType {
            invalid_type: 0x01,
        }));

        assert_eq!(actual, expected);
    }

    #[test]
    fn marshal_size() {
        let msg = Message::DataChannelAck;

        let actual = msg.marshal_size();
        let expected = 1;

        assert_eq!(actual, expected);
    }

    #[test]
    fn marshal() {
        let marshal_size = 12 + 5 + 8;
        let mut buf = BytesMut::with_capacity(marshal_size);

        let msg = Message::DataChannelOpen(DataChannelOpen {
            channel_type: ChannelType::Reliable,
            priority: 3893,
            reliability_parameter: 16715573,
            label: b"label".iter().cloned().collect(),
            protocol: b"protocol".iter().cloned().collect(),
        });

        let actual = msg.marshal_to(&mut buf).unwrap();
        let expected = marshal_size;
        assert_eq!(actual, expected);

        let bytes = buf.freeze();

        let actual = &bytes[..];
        let expected = &[
            0x03, // message type
            0x00, // channel type
            0x0f, 0x35, // priority
            0x00, 0xff, 0x0f, 0x35, // reliability parameter
            0x00, 0x05, // label length
            0x00, 0x08, // protocol length
            0x6c, 0x61, 0x62, 0x65, 0x6c, // label
            0x70, 0x72, 0x6f, 0x74, 0x6f, 0x63, 0x6f, 0x6c, // protocol
        ];

        assert_eq!(actual, expected);
    }
}
