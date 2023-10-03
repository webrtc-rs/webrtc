use std::io;
use std::string::FromUtf8Error;

use thiserror::Error;
use tokio::sync::mpsc::error::SendError as MpscSendError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    #[error("attribute not found")]
    ErrAttributeNotFound,
    #[error("transaction is stopped")]
    ErrTransactionStopped,
    #[error("transaction not exists")]
    ErrTransactionNotExists,
    #[error("transaction exists with same id")]
    ErrTransactionExists,
    #[error("agent is closed")]
    ErrAgentClosed,
    #[error("transaction is timed out")]
    ErrTransactionTimeOut,
    #[error("no default reason for ErrorCode")]
    ErrNoDefaultReason,
    #[error("unexpected EOF")]
    ErrUnexpectedEof,
    #[error("attribute size is invalid")]
    ErrAttributeSizeInvalid,
    #[error("attribute size overflow")]
    ErrAttributeSizeOverflow,
    #[error("attempt to decode to nil message")]
    ErrDecodeToNil,
    #[error("unexpected EOF: not enough bytes to read header")]
    ErrUnexpectedHeaderEof,
    #[error("integrity check failed")]
    ErrIntegrityMismatch,
    #[error("fingerprint check failed")]
    ErrFingerprintMismatch,
    #[error("FINGERPRINT before MESSAGE-INTEGRITY attribute")]
    ErrFingerprintBeforeIntegrity,
    #[error("bad UNKNOWN-ATTRIBUTES size")]
    ErrBadUnknownAttrsSize,
    #[error("invalid length of IP value")]
    ErrBadIpLength,
    #[error("no connection provided")]
    ErrNoConnection,
    #[error("client is closed")]
    ErrClientClosed,
    #[error("no agent is set")]
    ErrNoAgent,
    #[error("collector is closed")]
    ErrCollectorClosed,
    #[error("unsupported network")]
    ErrUnsupportedNetwork,
    #[error("invalid url")]
    ErrInvalidUrl,
    #[error("unknown scheme type")]
    ErrSchemeType,
    #[error("invalid hostname")]
    ErrHost,
    #[error("{0}")]
    Other(String),
    #[error("url parse: {0}")]
    Url(#[from] url::ParseError),
    #[error("utf8: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("{0}")]
    Io(#[source] IoError),
    #[error("mpsc send: {0}")]
    MpscSend(String),
    #[error("{0}")]
    Util(#[from] util::Error),
}

#[derive(Debug, Error)]
#[error("io error: {0}")]
pub struct IoError(#[from] pub io::Error);

// Workaround for wanting PartialEq for io::Error.
impl PartialEq for IoError {
    fn eq(&self, other: &Self) -> bool {
        self.0.kind() == other.0.kind()
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(IoError(e))
    }
}

// Because Tokio SendError is parameterized, we sadly lose the backtrace.
impl<T> From<MpscSendError<T>> for Error {
    fn from(e: MpscSendError<T>) -> Self {
        Error::MpscSend(e.to_string())
    }
}
