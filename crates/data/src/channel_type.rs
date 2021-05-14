use bytes::{Buf, BufMut};

use crate::marshal::{Marshal, MarshalSize, Unmarshal};

const CHANNEL_TYPE_RELIABLE: u8 = 0x00;
const CHANNEL_TYPE_RELIABLE_UNORDERED: u8 = 0x80;
const CHANNEL_TYPE_PARTIAL_RELIABLE_REXMIT: u8 = 0x01;
const CHANNEL_TYPE_PARTIAL_RELIABLE_REXMIT_UNORDERED: u8 = 0x81;
const CHANNEL_TYPE_PARTIAL_RELIABLE_TIMED: u8 = 0x02;
const CHANNEL_TYPE_PARTIAL_RELIABLE_TIMED_UNORDERED: u8 = 0x82;

const CHANNEL_TYPE_LEN: usize = 1;

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum Error {
    // Marshal buffer was too short
    UnexpectedEndOfBuffer { expected: usize, actual: usize },

    // Remote requested a channel type that we don't support
    InvalidChannelType { invalid_type: u8 },
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
            Self::InvalidChannelType { invalid_type } => {
                writeln!(f, "Invalid channel type: {:?}", invalid_type)
            }
        }
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum ChannelType {
    // `Reliable` determines the Data Channel provides a
    // reliable in-order bi-directional communication.
    Reliable,
    // `ReliableUnordered` determines the Data Channel
    // provides a reliable unordered bi-directional communication.
    ReliableUnordered,
    // `PartialReliableRexmit` determines the Data Channel
    // provides a partially-reliable in-order bi-directional communication.
    // User messages will not be retransmitted more times than specified in the Reliability Parameter.
    PartialReliableRexmit,
    // `PartialReliableRexmitUnordered` determines
    //  the Data Channel provides a partial reliable unordered bi-directional communication.
    // User messages will not be retransmitted more times than specified in the Reliability Parameter.
    PartialReliableRexmitUnordered,
    // `PartialReliableTimed` determines the Data Channel
    // provides a partial reliable in-order bi-directional communication.
    // User messages might not be transmitted or retransmitted after
    // a specified life-time given in milli- seconds in the Reliability Parameter.
    // This life-time starts when providing the user message to the protocol stack.
    PartialReliableTimed,
    // The Data Channel provides a partial reliable unordered bi-directional
    // communication.  User messages might not be transmitted or retransmitted
    // after a specified life-time given in milli- seconds in the Reliability Parameter.
    // This life-time starts when providing the user message to the protocol stack.
    PartialReliableTimedUnordered,
}

impl Default for ChannelType {
    fn default() -> Self {
        Self::Reliable
    }
}

impl MarshalSize for ChannelType {
    fn marshal_size(&self) -> usize {
        CHANNEL_TYPE_LEN
    }
}

impl Unmarshal for ChannelType {
    type Error = Error;

    fn unmarshal_from<B>(buf: &mut B) -> Result<Self, Self::Error>
    where
        Self: Sized,
        B: Buf,
    {
        let required_len = CHANNEL_TYPE_LEN;
        if buf.remaining() < required_len {
            return Err(Self::Error::UnexpectedEndOfBuffer {
                expected: required_len,
                actual: buf.remaining(),
            });
        }

        let b0 = buf.get_u8();

        match b0 {
            CHANNEL_TYPE_RELIABLE => Ok(Self::Reliable),
            CHANNEL_TYPE_RELIABLE_UNORDERED => Ok(Self::ReliableUnordered),
            CHANNEL_TYPE_PARTIAL_RELIABLE_REXMIT => Ok(Self::PartialReliableRexmit),
            CHANNEL_TYPE_PARTIAL_RELIABLE_REXMIT_UNORDERED => {
                Ok(Self::PartialReliableRexmitUnordered)
            }
            CHANNEL_TYPE_PARTIAL_RELIABLE_TIMED => Ok(Self::PartialReliableTimed),
            CHANNEL_TYPE_PARTIAL_RELIABLE_TIMED_UNORDERED => {
                Ok(Self::PartialReliableTimedUnordered)
            }
            _ => Err(Self::Error::InvalidChannelType { invalid_type: b0 }),
        }
    }
}

impl Marshal for ChannelType {
    type Error = Error;

    fn marshal_to<B>(&self, buf: &mut B) -> Result<usize, Self::Error>
    where
        B: BufMut,
    {
        let required_len = self.marshal_size();
        if buf.remaining_mut() < required_len {
            return Err(Self::Error::UnexpectedEndOfBuffer {
                expected: required_len,
                actual: buf.remaining_mut(),
            });
        }

        let byte = match self {
            Self::Reliable => CHANNEL_TYPE_RELIABLE,
            Self::ReliableUnordered => CHANNEL_TYPE_RELIABLE_UNORDERED,
            Self::PartialReliableRexmit => CHANNEL_TYPE_PARTIAL_RELIABLE_REXMIT,
            Self::PartialReliableRexmitUnordered => CHANNEL_TYPE_PARTIAL_RELIABLE_REXMIT_UNORDERED,
            Self::PartialReliableTimed => CHANNEL_TYPE_PARTIAL_RELIABLE_TIMED,
            Self::PartialReliableTimedUnordered => CHANNEL_TYPE_PARTIAL_RELIABLE_TIMED_UNORDERED,
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
    fn unmarshal_success() {
        let mut bytes = Bytes::from_static(&[0x00]);
        let channel_type = ChannelType::unmarshal_from(&mut bytes).unwrap();
        assert_eq!(channel_type, ChannelType::Reliable);
    }

    #[test]
    fn unmarshal_invalid_channel_type() {
        let mut bytes = Bytes::from_static(&[0x11]);
        let result = ChannelType::unmarshal_from(&mut bytes);
        assert_eq!(
            result,
            Err(Error::InvalidChannelType { invalid_type: 0x11 })
        );
    }

    #[test]
    fn unmarshal_unexpected_end_of_buffer() {
        let mut bytes = Bytes::from_static(&[]);
        let result = ChannelType::unmarshal_from(&mut bytes);
        assert_eq!(
            result,
            Err(Error::UnexpectedEndOfBuffer {
                expected: 1,
                actual: 0
            })
        );
    }

    #[test]
    fn marshal_size() {
        let channel_type = ChannelType::Reliable;

        let marshal_size = channel_type.marshal_size();

        assert_eq!(marshal_size, 1);
    }

    #[test]
    fn marshal() {
        let mut buf = BytesMut::with_capacity(1);
        let channel_type = ChannelType::Reliable;

        let bytes_written = channel_type.marshal_to(&mut buf).unwrap();
        assert_eq!(bytes_written, channel_type.marshal_size());

        let bytes = buf.freeze();
        assert_eq!(&bytes[..], &[0x00]);
    }
}
