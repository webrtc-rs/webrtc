use util::Error;

lazy_static! {
    // ErrAttributeNotFound means that attribute with provided attribute
    // type does not exist in message.
    pub static ref ERR_ATTRIBUTE_NOT_FOUND: Error = Error::new("attribute not found".to_owned());
    // ErrTransactionStopped indicates that transaction was manually stopped.
    pub static ref ERR_TRANSACTION_STOPPED: Error = Error::new("transaction is stopped".to_owned());
    // ErrTransactionNotExists indicates that agent failed to find transaction.
    pub static ref ERR_TRANSACTION_NOT_EXISTS: Error = Error::new("transaction not exists".to_owned());
    // ErrTransactionExists indicates that transaction with same id is already
    // registered.
    pub static ref ERR_TRANSACTION_EXISTS: Error = Error::new("transaction exists with same id".to_owned());
    // ErrAgentClosed indicates that agent is in closed state and is unable
    // to handle transactions.
    pub static ref ERR_AGENT_CLOSED: Error = Error::new("agent is closed".to_owned());
    // ErrTransactionTimeOut indicates that transaction has reached deadline.
    pub static ref ERR_TRANSACTION_TIME_OUT: Error = Error::new("transaction is timed out".to_owned());
    // ErrNoDefaultReason means that default reason for provided error code
    // is not defined in RFC.
    pub static ref ERR_NO_DEFAULT_REASON: Error = Error::new("no default reason for ErrorCode".to_owned());
    pub static ref ERR_UNEXPECTED_EOF: Error = Error::new("unexpected EOF".to_owned());
    // ErrAttributeSizeInvalid means that decoded attribute size is invalid.
    pub static ref ERR_ATTRIBUTE_SIZE_INVALID: Error = Error::new("attribute size is invalid".to_owned());
    // ErrAttributeSizeOverflow means that decoded attribute size is too big.
    pub static ref ERR_ATTRIBUTE_SIZE_OVERFLOW: Error = Error::new("attribute size overflow".to_owned());
    // ErrDecodeToNil occurs on Decode(data, nil) call.
    pub static ref ERR_DECODE_TO_NIL: Error = Error::new("attempt to decode to nil message".to_owned());
    // ErrUnexpectedHeaderEOF means that there were not enough bytes in Raw to read header.
    pub static ref ERR_UNEXPECTED_HEADER_EOF: Error = Error::new("unexpected EOF: not enough bytes to read header".to_owned());
    // ErrIntegrityMismatch means that computed HMAC differs from expected.
    pub static ref ERR_INTEGRITY_MISMATCH: Error = Error::new("integrity check failed".to_owned());
    // ErrFingerprintMismatch means that computed fingerprint differs from expected.
    pub static ref ERR_FINGERPRINT_MISMATCH: Error = Error::new("fingerprint check failed".to_owned());
    // ErrFingerprintBeforeIntegrity means that FINGERPRINT attribute is already in
    // message, so MESSAGE-INTEGRITY attribute cannot be added.
    pub static ref ERR_FINGERPRINT_BEFORE_INTEGRITY: Error = Error::new("FINGERPRINT before MESSAGE-INTEGRITY attribute".to_owned());
    // ErrBadUnknownAttrsSize means that UNKNOWN-ATTRIBUTES attribute value
    // has invalid length.
    pub static ref ERR_BAD_UNKNOWN_ATTRS_SIZE: Error = Error::new("bad UNKNOWN-ATTRIBUTES size".to_owned());
    // ErrBadIPLength means that len(IP) is not net.{IPv6len,IPv4len}.
    pub static ref ERR_BAD_IP_LENGTH: Error = Error::new("invalid length of IP value".to_owned());
    // ErrNoConnection means that ClientOptions.Connection is nil.
    pub static ref ERR_NO_CONNECTION: Error = Error::new("no connection provided".to_owned());
    // ErrClientClosed indicates that client is closed.
    pub static ref ERR_CLIENT_CLOSED: Error = Error::new("client is closed".to_owned());
    // ErrNoAgent indicates that agent is not set.
    pub static ref ERR_NO_AGENT: Error = Error::new("no agent is set".to_owned());
    // ErrCollectorClosed indicates that client is closed.
    pub static ref ERR_COLLECTOR_CLOSED: Error = Error::new("collector is closed".to_owned());
    // ErrUnsupportedNetwork indicates that client is closed.
    pub static ref ERR_UNSUPPORTED_NETWORK: Error = Error::new("unsupported network".to_owned());
    pub static ref ERR_INVALID_URL: Error = Error::new("invalid url".to_owned());
    // ErrSchemeType indicates the scheme type could not be parsed.
    pub static ref ERR_SCHEME_TYPE:Error = Error::new("unknown scheme type".to_owned());
    // ErrHost indicates malformed hostname is provided.
    pub static ref ERR_HOST:Error = Error::new("invalid hostname".to_owned());
}
