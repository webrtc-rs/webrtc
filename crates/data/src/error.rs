use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("SctpError: {0}")]
    ErrSctpError(#[from] sctp::error::Error),

    #[error(
        "DataChannel message is not long enough to determine type: (expected: {expected}, actual: {actual})"
    )]
    UnexpectedEndOfBuffer { expected: usize, actual: usize },
    #[error("Unknown MessageType {0}")]
    InvalidMessageType(u8),
}
