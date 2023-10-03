use super::*;

type Result<T> = std::result::Result<T, util::Error>;

/// The data-part of an data-channel ACK message without the message type.
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

impl Marshal for DataChannelAck {
    fn marshal_to(&self, _buf: &mut [u8]) -> Result<usize> {
        Ok(0)
    }
}

impl Unmarshal for DataChannelAck {
    fn unmarshal<B>(_buf: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};

    use super::*;

    #[test]
    fn test_channel_ack_unmarshal() -> Result<()> {
        let mut bytes = Bytes::from_static(&[]);

        let channel_ack = DataChannelAck::unmarshal(&mut bytes)?;

        assert_eq!(channel_ack, DataChannelAck);
        Ok(())
    }

    #[test]
    fn test_channel_ack_marshal_size() -> Result<()> {
        let channel_ack = DataChannelAck;
        let marshal_size = channel_ack.marshal_size();

        assert_eq!(marshal_size, 0);
        Ok(())
    }

    #[test]
    fn test_channel_ack_marshal() -> Result<()> {
        let channel_ack = DataChannelAck;
        let mut buf = BytesMut::with_capacity(0);
        let bytes_written = channel_ack.marshal_to(&mut buf)?;
        let bytes = buf.freeze();

        assert_eq!(bytes_written, channel_ack.marshal_size());
        assert_eq!(&bytes[..], &[]);
        Ok(())
    }
}
