use bytes::Bytes;

/// DataChannelMessage represents a message received from the
/// data channel. IsString will be set to true if the incoming
/// message is of the string type. Otherwise the message is of
/// a binary type.
#[derive(Default, Debug, Clone)]
pub struct DataChannelMessage {
    pub is_string: bool,
    pub data: Bytes,
}
