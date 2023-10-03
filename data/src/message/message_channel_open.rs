use super::*;
use crate::error::Error;

type Result<T> = std::result::Result<T, util::Error>;

const CHANNEL_TYPE_RELIABLE: u8 = 0x00;
const CHANNEL_TYPE_RELIABLE_UNORDERED: u8 = 0x80;
const CHANNEL_TYPE_PARTIAL_RELIABLE_REXMIT: u8 = 0x01;
const CHANNEL_TYPE_PARTIAL_RELIABLE_REXMIT_UNORDERED: u8 = 0x81;
const CHANNEL_TYPE_PARTIAL_RELIABLE_TIMED: u8 = 0x02;
const CHANNEL_TYPE_PARTIAL_RELIABLE_TIMED_UNORDERED: u8 = 0x82;
const CHANNEL_TYPE_LEN: usize = 1;

/// ChannelPriority
pub const CHANNEL_PRIORITY_BELOW_NORMAL: u16 = 128;
pub const CHANNEL_PRIORITY_NORMAL: u16 = 256;
pub const CHANNEL_PRIORITY_HIGH: u16 = 512;
pub const CHANNEL_PRIORITY_EXTRA_HIGH: u16 = 1024;

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

const CHANNEL_OPEN_HEADER_LEN: usize = 11;

/// The data-part of an data-channel OPEN message without the message type.
///
/// # Memory layout
///
/// ```plain
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// | (Message Type)|  Channel Type |            Priority           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                    Reliability Parameter                      |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |         Label Length          |       Protocol Length         |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                                                               |
/// |                             Label                             |
/// |                                                               |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                                                               |
/// |                            Protocol                           |
/// |                                                               |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// ```
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct DataChannelOpen {
    pub channel_type: ChannelType,
    pub priority: u16,
    pub reliability_parameter: u32,
    pub label: Vec<u8>,
    pub protocol: Vec<u8>,
}

impl MarshalSize for DataChannelOpen {
    fn marshal_size(&self) -> usize {
        let label_len = self.label.len();
        let protocol_len = self.protocol.len();

        CHANNEL_OPEN_HEADER_LEN + label_len + protocol_len
    }
}

impl Marshal for DataChannelOpen {
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        let required_len = self.marshal_size();
        if buf.remaining_mut() < required_len {
            return Err(Error::UnexpectedEndOfBuffer {
                expected: required_len,
                actual: buf.remaining_mut(),
            }
            .into());
        }

        let n = self.channel_type.marshal_to(buf)?;
        buf = &mut buf[n..];
        buf.put_u16(self.priority);
        buf.put_u32(self.reliability_parameter);
        buf.put_u16(self.label.len() as u16);
        buf.put_u16(self.protocol.len() as u16);
        buf.put_slice(self.label.as_slice());
        buf.put_slice(self.protocol.as_slice());
        Ok(self.marshal_size())
    }
}

impl Unmarshal for DataChannelOpen {
    fn unmarshal<B>(buf: &mut B) -> Result<Self>
    where
        B: Buf,
    {
        let required_len = CHANNEL_OPEN_HEADER_LEN;
        if buf.remaining() < required_len {
            return Err(Error::UnexpectedEndOfBuffer {
                expected: required_len,
                actual: buf.remaining(),
            }
            .into());
        }

        let channel_type = ChannelType::unmarshal(buf)?;
        let priority = buf.get_u16();
        let reliability_parameter = buf.get_u32();
        let label_len = buf.get_u16() as usize;
        let protocol_len = buf.get_u16() as usize;

        let required_len = label_len + protocol_len;
        if buf.remaining() < required_len {
            return Err(Error::UnexpectedEndOfBuffer {
                expected: required_len,
                actual: buf.remaining(),
            }
            .into());
        }

        let mut label = vec![0; label_len];
        let mut protocol = vec![0; protocol_len];

        buf.copy_to_slice(&mut label[..]);
        buf.copy_to_slice(&mut protocol[..]);

        Ok(Self {
            channel_type,
            priority,
            reliability_parameter,
            label,
            protocol,
        })
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};

    use super::*;

    #[test]
    fn test_channel_type_unmarshal_success() -> Result<()> {
        let mut bytes = Bytes::from_static(&[0x00]);
        let channel_type = ChannelType::unmarshal(&mut bytes)?;

        assert_eq!(channel_type, ChannelType::Reliable);
        Ok(())
    }

    #[test]
    fn test_channel_type_unmarshal_invalid() -> Result<()> {
        let mut bytes = Bytes::from_static(&[0x11]);
        match ChannelType::unmarshal(&mut bytes) {
            Ok(_) => panic!("expected Error, but got Ok"),
            Err(err) => {
                if let Some(&Error::InvalidChannelType(0x11)) = err.downcast_ref::<Error>() {
                    return Ok(());
                }
                panic!(
                    "unexpected err {:?}, want {:?}",
                    err,
                    Error::InvalidMessageType(0x01)
                );
            }
        }
    }

    #[test]
    fn test_channel_type_unmarshal_unexpected_end_of_buffer() -> Result<()> {
        let mut bytes = Bytes::from_static(&[]);
        match ChannelType::unmarshal(&mut bytes) {
            Ok(_) => panic!("expected Error, but got Ok"),
            Err(err) => {
                if let Some(&Error::UnexpectedEndOfBuffer {
                    expected: 1,
                    actual: 0,
                }) = err.downcast_ref::<Error>()
                {
                    return Ok(());
                }
                panic!(
                    "unexpected err {:?}, want {:?}",
                    err,
                    Error::InvalidMessageType(0x01)
                );
            }
        }
    }

    #[test]
    fn test_channel_type_marshal_size() -> Result<()> {
        let channel_type = ChannelType::Reliable;
        let marshal_size = channel_type.marshal_size();

        assert_eq!(marshal_size, 1);
        Ok(())
    }

    #[test]
    fn test_channel_type_marshal() -> Result<()> {
        let mut buf = BytesMut::with_capacity(1);
        buf.resize(1, 0u8);
        let channel_type = ChannelType::Reliable;
        let bytes_written = channel_type.marshal_to(&mut buf)?;
        assert_eq!(bytes_written, channel_type.marshal_size());

        let bytes = buf.freeze();
        assert_eq!(&bytes[..], &[0x00]);
        Ok(())
    }

    static MARSHALED_BYTES: [u8; 24] = [
        0x00, // channel type
        0x0f, 0x35, // priority
        0x00, 0xff, 0x0f, 0x35, // reliability parameter
        0x00, 0x05, // label length
        0x00, 0x08, // protocol length
        0x6c, 0x61, 0x62, 0x65, 0x6c, // label
        0x70, 0x72, 0x6f, 0x74, 0x6f, 0x63, 0x6f, 0x6c, // protocol
    ];

    #[test]
    fn test_channel_open_unmarshal_success() -> Result<()> {
        let mut bytes = Bytes::from_static(&MARSHALED_BYTES);

        let channel_open = DataChannelOpen::unmarshal(&mut bytes)?;

        assert_eq!(channel_open.channel_type, ChannelType::Reliable);
        assert_eq!(channel_open.priority, 3893);
        assert_eq!(channel_open.reliability_parameter, 16715573);
        assert_eq!(channel_open.label, b"label");
        assert_eq!(channel_open.protocol, b"protocol");
        Ok(())
    }

    #[test]
    fn test_channel_open_unmarshal_invalid_channel_type() -> Result<()> {
        let mut bytes = Bytes::from_static(&[
            0x11, // channel type
            0x0f, 0x35, // priority
            0x00, 0xff, 0x0f, 0x35, // reliability parameter
            0x00, 0x05, // label length
            0x00, 0x08, // protocol length
        ]);
        match DataChannelOpen::unmarshal(&mut bytes) {
            Ok(_) => panic!("expected Error, but got Ok"),
            Err(err) => {
                if let Some(&Error::InvalidChannelType(0x11)) = err.downcast_ref::<Error>() {
                    return Ok(());
                }
                panic!(
                    "unexpected err {:?}, want {:?}",
                    err,
                    Error::InvalidMessageType(0x01)
                );
            }
        }
    }

    #[test]
    fn test_channel_open_unmarshal_unexpected_end_of_buffer() -> Result<()> {
        let mut bytes = Bytes::from_static(&[0x00; 5]);
        match DataChannelOpen::unmarshal(&mut bytes) {
            Ok(_) => panic!("expected Error, but got Ok"),
            Err(err) => {
                if let Some(&Error::UnexpectedEndOfBuffer {
                    expected: 11,
                    actual: 5,
                }) = err.downcast_ref::<Error>()
                {
                    return Ok(());
                }
                panic!(
                    "unexpected err {:?}, want {:?}",
                    err,
                    Error::InvalidMessageType(0x01)
                );
            }
        }
    }

    #[test]
    fn test_channel_open_unmarshal_unexpected_length_mismatch() -> Result<()> {
        let mut bytes = Bytes::from_static(&[
            0x01, // channel type
            0x00, 0x00, // priority
            0x00, 0x00, 0x00, 0x00, // Reliability parameter
            0x00, 0x05, // Label length
            0x00, 0x08, // Protocol length
        ]);
        match DataChannelOpen::unmarshal(&mut bytes) {
            Ok(_) => panic!("expected Error, but got Ok"),
            Err(err) => {
                if let Some(&Error::UnexpectedEndOfBuffer {
                    expected: 13,
                    actual: 0,
                }) = err.downcast_ref::<Error>()
                {
                    return Ok(());
                }
                panic!(
                    "unexpected err {:?}, want {:?}",
                    err,
                    Error::InvalidMessageType(0x01)
                );
            }
        }
    }

    #[test]
    fn test_channel_open_marshal_size() -> Result<()> {
        let channel_open = DataChannelOpen {
            channel_type: ChannelType::Reliable,
            priority: 3893,
            reliability_parameter: 16715573,
            label: b"label".to_vec(),
            protocol: b"protocol".to_vec(),
        };

        let marshal_size = channel_open.marshal_size();

        assert_eq!(marshal_size, 11 + 5 + 8);
        Ok(())
    }

    #[test]
    fn test_channel_open_marshal() -> Result<()> {
        let channel_open = DataChannelOpen {
            channel_type: ChannelType::Reliable,
            priority: 3893,
            reliability_parameter: 16715573,
            label: b"label".to_vec(),
            protocol: b"protocol".to_vec(),
        };

        let mut buf = BytesMut::with_capacity(11 + 5 + 8);
        buf.resize(11 + 5 + 8, 0u8);
        let bytes_written = channel_open.marshal_to(&mut buf).unwrap();
        let bytes = buf.freeze();

        assert_eq!(bytes_written, channel_open.marshal_size());
        assert_eq!(&bytes[..], &MARSHALED_BYTES);
        Ok(())
    }
}
