use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

#[derive(Debug, Error)] //PartialEq, Clone
pub enum Error {
    #[error("conn is closed")]
    ERR_CONN_CLOSED,
    #[error("read/write timeout")]
    ERR_DEADLINE_EXCEEDED,
    #[error("buffer is too small")]
    ERR_BUFFER_TOO_SMALL,
    #[error("context is not supported for export_keying_material")]
    ERR_CONTEXT_UNSUPPORTED,
    #[error("packet is too short")]
    ERR_DTLSPACKET_INVALID_LENGTH,
    #[error("handshake is in progress")]
    ERR_HANDSHAKE_IN_PROGRESS,
    #[error("invalid content type")]
    ERR_INVALID_CONTENT_TYPE,
    #[error("invalid mac")]
    ERR_INVALID_MAC,
    #[error("packet length and declared length do not match")]
    ERR_INVALID_PACKET_LENGTH,
    #[error("export_keying_material can not be used with a reserved label")]
    ERR_RESERVED_EXPORT_KEYING_MATERIAL,
    #[error("client sent certificate verify but we have no certificate to verify")]
    ERR_CERTIFICATE_VERIFY_NO_CERTIFICATE,
    #[error("client+server do not support any shared cipher suites")]
    ERR_CIPHER_SUITE_NO_INTERSECTION,
    #[error("server hello can not be created without a cipher suite")]
    ERR_CIPHER_SUITE_UNSET,
    #[error("client sent certificate but did not verify it")]
    ERR_CLIENT_CERTIFICATE_NOT_VERIFIED,
    #[error("server required client verification, but got none")]
    ERR_CLIENT_CERTIFICATE_REQUIRED,
    #[error("server responded with SRTP Profile we do not support")]
    ERR_CLIENT_NO_MATCHING_SRTP_PROFILE,
    #[error("client required Extended Master Secret extension, but server does not support it")]
    ERR_CLIENT_REQUIRED_BUT_NO_SERVER_EMS,
    #[error("server hello can not be created without a compression method")]
    ERR_COMPRESSION_METHOD_UNSET,
    #[error("client+server cookie does not match")]
    ERR_COOKIE_MISMATCH,
    #[error("cookie must not be longer then 255 bytes")]
    ERR_COOKIE_TOO_LONG,
    #[error("PSK Identity Hint provided but PSK is nil")]
    ERR_IDENTITY_NO_PSK,
    #[error("no certificate provided")]
    ERR_INVALID_CERTIFICATE,
    #[error("cipher spec invalid")]
    ERR_INVALID_CIPHER_SPEC,
    #[error("invalid or unknown cipher suite")]
    ERR_INVALID_CIPHER_SUITE,
    #[error("unable to determine if ClientKeyExchange is a public key or PSK Identity")]
    ERR_INVALID_CLIENT_KEY_EXCHANGE,
    #[error("invalid or unknown compression method")]
    ERR_INVALID_COMPRESSION_METHOD,
    #[error("ECDSA signature contained zero or negative values")]
    ERR_INVALID_ECDSASIGNATURE,
    #[error("invalid or unknown elliptic curve type")]
    ERR_INVALID_ELLIPTIC_CURVE_TYPE,
    #[error("invalid extension type")]
    ERR_INVALID_EXTENSION_TYPE,
    #[error("invalid hash algorithm")]
    ERR_INVALID_HASH_ALGORITHM,
    #[error("invalid named curve")]
    ERR_INVALID_NAMED_CURVE,
    #[error("invalid private key type")]
    ERR_INVALID_PRIVATE_KEY,
    #[error("named curve and private key type does not match")]
    ERR_NAMED_CURVE_AND_PRIVATE_KEY_MISMATCH,
    #[error("invalid server name format")]
    ERR_INVALID_SNI_FORMAT,
    #[error("invalid signature algorithm")]
    ERR_INVALID_SIGNATURE_ALGORITHM,
    #[error("expected and actual key signature do not match")]
    ERR_KEY_SIGNATURE_MISMATCH,
    #[error("Conn can not be created with a nil nextConn")]
    ERR_NIL_NEXT_CONN,
    #[error("connection can not be created, no CipherSuites satisfy this Config")]
    ERR_NO_AVAILABLE_CIPHER_SUITES,
    #[error("connection can not be created, no SignatureScheme satisfy this Config")]
    ERR_NO_AVAILABLE_SIGNATURE_SCHEMES,
    #[error("no certificates configured")]
    ERR_NO_CERTIFICATES,
    #[error("no config provided")]
    ERR_NO_CONFIG_PROVIDED,
    #[error("client requested zero or more elliptic curves that are not supported by the server")]
    ERR_NO_SUPPORTED_ELLIPTIC_CURVES,
    #[error("unsupported protocol version")]
    ERR_UNSUPPORTED_PROTOCOL_VERSION,
    #[error("Certificate and PSK provided")]
    ERR_PSK_AND_CERTIFICATE,
    #[error("PSK and PSK Identity Hint must both be set for client")]
    ERR_PSK_AND_IDENTITY_MUST_BE_SET_FOR_CLIENT,
    #[error("SRTP support was requested but server did not respond with use_srtp extension")]
    ERR_REQUESTED_BUT_NO_SRTP_EXTENSION,
    #[error("Certificate is mandatory for server")]
    ERR_SERVER_MUST_HAVE_CERTIFICATE,
    #[error("client requested SRTP but we have no matching profiles")]
    ERR_SERVER_NO_MATCHING_SRTP_PROFILE,
    #[error(
        "server requires the Extended Master Secret extension, but the client does not support it"
    )]
    ERR_SERVER_REQUIRED_BUT_NO_CLIENT_EMS,
    #[error("expected and actual verify data does not match")]
    ERR_VERIFY_DATA_MISMATCH,
    #[error("handshake message unset, unable to marshal")]
    ERR_HANDSHAKE_MESSAGE_UNSET,
    #[error("invalid flight number")]
    ERR_INVALID_FLIGHT,
    #[error("unable to generate key signature, unimplemented")]
    ERR_KEY_SIGNATURE_GENERATE_UNIMPLEMENTED,
    #[error("unable to verify key signature, unimplemented")]
    ERR_KEY_SIGNATURE_VERIFY_UNIMPLEMENTED,
    #[error("data length and declared length do not match")]
    ERR_LENGTH_MISMATCH,
    #[error("buffer not long enough to contain nonce")]
    ERR_NOT_ENOUGH_ROOM_FOR_NONCE,
    #[error("feature has not been implemented yet")]
    ERR_NOT_IMPLEMENTED,
    #[error("sequence number overflow")]
    ERR_SEQUENCE_NUMBER_OVERFLOW,
    #[error("unable to marshal fragmented handshakes")]
    ERR_UNABLE_TO_MARSHAL_FRAGMENTED,
    #[error("invalid state machine transition")]
    ERR_INVALID_FSM_TRANSITION,
    #[error("ApplicationData with epoch of 0")]
    ERR_APPLICATION_DATA_EPOCH_ZERO,
    #[error("unhandled contentType")]
    ERR_UNHANDLED_CONTEXT_TYPE,
    #[error("context canceled")]
    ERR_CONTEXT_CANCELED,
    #[error("empty fragment")]
    ERR_EMPTY_FRAGMENT,
    #[error("Alert is Fatal or Close Notify")]
    ERR_ALERT_FATAL_OR_CLOSE,

    #[error("Other errors:{0}")]
    ErrOthers(String),

    #[error("IoError: {0}")]
    ErrIoError(#[from] std::io::Error),
    #[error("P256Error: {0}")]
    ErrP256Error(#[from] p256::elliptic_curve::Error),
    #[error("TryFromSliceError: {0}")]
    ErrTryFromSliceError(#[from] std::array::TryFromSliceError),
    #[error("InvalidKeyLengthError: {0}")]
    ErrInvalidKeyLengthError(#[from] hmac::crypto_mac::InvalidKeyLength),
    #[error("FromUtf8Error: {0}")]
    ErrFromUtf8Error(#[from] std::string::FromUtf8Error),
    #[error("AesGcmError: {0}")]
    ErrAesGcmError(#[from] aes_gcm::Error),
    #[error("InvalidKeyIvLengthError: {0}")]
    ErrInvalidKeyIvLengthError(#[from] block_modes::InvalidKeyIvLength),
    #[error("BlockModeErrorError: {0}")]
    ErrBlockModeErrorError(#[from] block_modes::BlockModeError),
    #[error("RcgenError: {0}")]
    ErrRcgenError(#[from] rcgen::RcgenError),
    #[error("X509Error: {0}")]
    ErrX509Error(#[from] der_parser::nom::Err<x509_parser::error::X509Error>),
}

impl<T> From<SendError<T>> for Error {
    fn from(error: SendError<T>) -> Self {
        Error::ErrOthers(error.to_string())
    }
}

impl From<ring::error::KeyRejected> for Error {
    fn from(error: ring::error::KeyRejected) -> Self {
        Error::ErrOthers(error.to_string())
    }
}

impl From<ring::error::Unspecified> for Error {
    fn from(error: ring::error::Unspecified) -> Self {
        Error::ErrOthers(error.to_string())
    }
}
