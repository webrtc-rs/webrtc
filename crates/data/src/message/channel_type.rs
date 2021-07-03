use super::*;
use crate::error::Error;

const CHANNEL_TYPE_RELIABLE: u8 = 0x00;
const CHANNEL_TYPE_RELIABLE_UNORDERED: u8 = 0x80;
const CHANNEL_TYPE_PARTIAL_RELIABLE_REXMIT: u8 = 0x01;
const CHANNEL_TYPE_PARTIAL_RELIABLE_REXMIT_UNORDERED: u8 = 0x81;
const CHANNEL_TYPE_PARTIAL_RELIABLE_TIMED: u8 = 0x02;
const CHANNEL_TYPE_PARTIAL_RELIABLE_TIMED_UNORDERED: u8 = 0x82;
const CHANNEL_TYPE_LEN: usize = 1;

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

impl Marshal for ChannelType {
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        let required_len = self.marshal_size();
        if buf.remaining_mut() < required_len {
            return Err(Error::UnexpectedEndOfBuffer {
                expected: required_len,
                actual: buf.remaining_mut(),
            }
            .into());
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

impl Unmarshal for ChannelType {
    fn unmarshal<B>(buf: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        let required_len = CHANNEL_TYPE_LEN;
        if buf.remaining() < required_len {
            return Err(Error::UnexpectedEndOfBuffer {
                expected: required_len,
                actual: buf.remaining(),
            }
            .into());
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
            _ => Err(Error::InvalidChannelType(b0).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::{Bytes, BytesMut};

    #[test]
    fn unmarshal_success() -> Result<()> {
        let mut bytes = Bytes::from_static(&[0x00]);
        let channel_type = ChannelType::unmarshal(&mut bytes)?;

        assert_eq!(channel_type, ChannelType::Reliable);
        Ok(())
    }

    #[test]
    fn unmarshal_invalid_channel_type() -> Result<()> {
        let mut bytes = Bytes::from_static(&[0x11]);
        match ChannelType::unmarshal(&mut bytes) {
            Ok(_) => assert!(false, "expected Error, but got Ok"),
            Err(err) => {
                if let Some(err) = err.downcast_ref::<Error>() {
                    match err {
                        &Error::InvalidChannelType(0x11) => return Ok(()),
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
    fn unmarshal_unexpected_end_of_buffer() -> Result<()> {
        let mut bytes = Bytes::from_static(&[]);
        match ChannelType::unmarshal(&mut bytes) {
            Ok(_) => assert!(false, "expected Error, but got Ok"),
            Err(err) => {
                if let Some(err) = err.downcast_ref::<Error>() {
                    match err {
                        &Error::UnexpectedEndOfBuffer {
                            expected: 1,
                            actual: 0,
                        } => return Ok(()),
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
        let channel_type = ChannelType::Reliable;
        let marshal_size = channel_type.marshal_size();

        assert_eq!(marshal_size, 1);
        Ok(())
    }

    #[test]
    fn marshal() -> Result<()> {
        let mut buf = BytesMut::with_capacity(1);
        buf.resize(1, 0u8);
        let channel_type = ChannelType::Reliable;
        let bytes_written = channel_type.marshal_to(&mut buf)?;
        assert_eq!(bytes_written, channel_type.marshal_size());

        let bytes = buf.freeze();
        assert_eq!(&bytes[..], &[0x00]);
        Ok(())
    }
}
