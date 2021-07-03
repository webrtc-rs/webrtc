use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error(
        "DataChannel message is not long enough to determine type: (expected: {expected}, actual: {actual})"
    )]
    UnexpectedEndOfBuffer { expected: usize, actual: usize },
    #[error("Unknown MessageType {0}")]
    InvalidMessageType(u8),
    #[error("Unknown ChannelType {0}")]
    InvalidChannelType(u8),
    #[error("Unknown PayloadProtocolIdentifier {0}")]
    InvalidPayloadProtocolIdentifier(u8),

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        err.downcast_ref::<Self>().map_or(false, |e| e == self)
    }
}
