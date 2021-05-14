use bytes::{Buf, BufMut};

use crate::{
    error::DataChannelAckError,
    marshal::{Marshal, MarshalSize, Unmarshal},
};

/// The data-part of an data-channel OPEN message without the message type.
///
/// # Memory layout
///
/// ```plain
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|  Message Type |
///+-+-+-+-+-+-+-+-+
/// ```
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct DataChannelAck;

impl MarshalSize for DataChannelAck {
    fn marshal_size(&self) -> usize {
        0
    }
}

impl Unmarshal for DataChannelAck {
    type Error = DataChannelAckError;

    fn unmarshal_from<B>(_buf: &mut B) -> Result<Self, Self::Error>
    where
        B: Buf,
    {
        Ok(Self)
    }
}

impl Marshal for DataChannelAck {
    type Error = DataChannelAckError;

    fn marshal_to<B>(&self, _buf: &mut B) -> Result<usize, Self::Error>
    where
        B: BufMut,
    {
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};

    use super::*;

    #[test]
    fn unmarshal() {
        let mut bytes = Bytes::from_static(&[]);

        let data_channel_ack = DataChannelAck::unmarshal_from(&mut bytes).unwrap();

        assert_eq!(data_channel_ack, DataChannelAck);
    }

    #[test]
    fn marshal_size() {
        let data_channel_ack = DataChannelAck;

        let marshal_size = data_channel_ack.marshal_size();

        assert_eq!(marshal_size, 0);
    }

    #[test]
    fn marshal() {
        let data_channel_ack = DataChannelAck;

        let mut buf = BytesMut::with_capacity(0);
        let bytes_written = data_channel_ack.marshal_to(&mut buf).unwrap();
        let bytes = buf.freeze();

        assert_eq!(bytes_written, data_channel_ack.marshal_size());
        assert_eq!(&bytes[..], &[]);
    }
}
