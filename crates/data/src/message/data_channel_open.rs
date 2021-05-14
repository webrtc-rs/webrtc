use bytes::{Buf, BufMut};

use crate::{
    channel_type::ChannelType,
    error::DataChannelOpenError,
    marshal::{Marshal, MarshalSize, Unmarshal},
};

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

impl Unmarshal for DataChannelOpen {
    type Error = DataChannelOpenError;

    fn unmarshal_from<B>(buf: &mut B) -> Result<Self, Self::Error>
    where
        B: Buf,
    {
        let required_len = CHANNEL_OPEN_HEADER_LEN;
        if buf.remaining() < required_len {
            return Err(Self::Error::UnexpectedEndOfBuffer {
                expected: required_len,
                actual: buf.remaining(),
            });
        }

        let channel_type = ChannelType::unmarshal_from(buf)?;
        let priority = buf.get_u16();
        let reliability_parameter = buf.get_u32();
        let label_len = buf.get_u16() as usize;
        let protocol_len = buf.get_u16() as usize;

        let required_len = label_len + protocol_len;
        if buf.remaining() < required_len {
            return Err(Self::Error::ExpectedAndActualLengthMismatch {
                expected: required_len,
                actual: buf.remaining(),
            });
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

impl Marshal for DataChannelOpen {
    type Error = DataChannelOpenError;

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

        self.channel_type.marshal_to(buf)?;
        buf.put_u16(self.priority);
        buf.put_u32(self.reliability_parameter);
        buf.put_u16(self.label.len() as u16);
        buf.put_u16(self.protocol.len() as u16);
        buf.put_slice(self.label.as_slice());
        buf.put_slice(self.protocol.as_slice());
        Ok(self.marshal_size())
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};

    use crate::error::ChannelTypeError;

    use super::*;

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
    fn unmarshal_success() {
        let mut bytes = Bytes::from_static(&MARSHALED_BYTES);

        let data_channel_open = DataChannelOpen::unmarshal_from(&mut bytes).unwrap();

        assert_eq!(data_channel_open.channel_type, ChannelType::Reliable);
        assert_eq!(data_channel_open.priority, 3893);
        assert_eq!(data_channel_open.reliability_parameter, 16715573);
        assert_eq!(data_channel_open.label, b"label");
        assert_eq!(data_channel_open.protocol, b"protocol");
    }

    #[test]
    fn unmarshal_invalid_channel_type() {
        let mut bytes = Bytes::from_static(&[
            0x11, // channel type
            0x0f, 0x35, // priority
            0x00, 0xff, 0x0f, 0x35, // reliability parameter
            0x00, 0x05, // label length
            0x00, 0x08, // protocol length
        ]);
        let result = DataChannelOpen::unmarshal_from(&mut bytes);
        assert_eq!(
            result,
            Err(DataChannelOpenError::ChannelType(
                ChannelTypeError::InvalidChannelType { invalid_type: 0x11 }
            ))
        );
    }

    #[test]
    fn unmarshal_unexpected_end_of_buffer() {
        let mut bytes = Bytes::from_static(&[0x00; 5]);
        let result = DataChannelOpen::unmarshal_from(&mut bytes);
        assert_eq!(
            result,
            Err(DataChannelOpenError::UnexpectedEndOfBuffer {
                expected: 11,
                actual: 5
            })
        );
    }

    #[test]
    fn unmarshal_unexpected_length_mismatch() {
        let mut bytes = Bytes::from_static(&[
            0x01, // channel type
            0x00, 0x00, // priority
            0x00, 0x00, 0x00, 0x00, // Reliability parameter
            0x00, 0x05, // Label length
            0x00, 0x08, // Protocol length
        ]);
        let result = DataChannelOpen::unmarshal_from(&mut bytes);
        assert_eq!(
            result,
            Err(DataChannelOpenError::ExpectedAndActualLengthMismatch {
                expected: 5 + 8,
                actual: 0
            })
        );
    }

    #[test]
    fn marshal_size() {
        let data_channel_open = DataChannelOpen {
            channel_type: ChannelType::Reliable,
            priority: 3893,
            reliability_parameter: 16715573,
            label: b"label".iter().cloned().collect(),
            protocol: b"protocol".iter().cloned().collect(),
        };

        let marshal_size = data_channel_open.marshal_size();

        assert_eq!(marshal_size, 11 + 5 + 8);
    }

    #[test]
    fn marshal() {
        let data_channel_open = DataChannelOpen {
            channel_type: ChannelType::Reliable,
            priority: 3893,
            reliability_parameter: 16715573,
            label: b"label".iter().cloned().collect(),
            protocol: b"protocol".iter().cloned().collect(),
        };

        let mut buf = BytesMut::with_capacity(11 + 5 + 8);
        let bytes_written = data_channel_open.marshal_to(&mut buf).unwrap();
        let bytes = buf.freeze();

        assert_eq!(bytes_written, data_channel_open.marshal_size());
        assert_eq!(&bytes[..], &MARSHALED_BYTES);
    }
}
