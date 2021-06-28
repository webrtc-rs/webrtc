use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
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

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        err.downcast_ref::<Self>().map_or(false, |e| e == self)
    }
}
