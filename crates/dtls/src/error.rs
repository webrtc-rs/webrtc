use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

#[derive(Debug, Error)] //PartialEq, Clone
pub enum Error {
    #[error("conn is closed")]
    ErrConnClosed,
    #[error("read/write timeout")]
    ErrDeadlineExceeded,
    #[error("buffer is too small")]
    ErrBufferTooSmall,
    #[error("context is not supported for export_keying_material")]
    ErrContextUnsupported,
    #[error("packet is too short")]
    ErrDtlspacketInvalidLength,
    #[error("handshake is in progress")]
    ErrHandshakeInProgress,
    #[error("invalid content type")]
    ErrInvalidContentType,
    #[error("invalid mac")]
    ErrInvalidMac,
    #[error("packet length and declared length do not match")]
    ErrInvalidPacketLength,
    #[error("export_keying_material can not be used with a reserved label")]
    ErrReservedExportKeyingMaterial,
    #[error("client sent certificate verify but we have no certificate to verify")]
    ErrCertificateVerifyNoCertificate,
    #[error("client+server do not support any shared cipher suites")]
    ErrCipherSuiteNoIntersection,
    #[error("server hello can not be created without a cipher suite")]
    ErrCipherSuiteUnset,
    #[error("client sent certificate but did not verify it")]
    ErrClientCertificateNotVerified,
    #[error("server required client verification, but got none")]
    ErrClientCertificateRequired,
    #[error("server responded with SRTP Profile we do not support")]
    ErrClientNoMatchingSrtpProfile,
    #[error("client required Extended Master Secret extension, but server does not support it")]
    ErrClientRequiredButNoServerEms,
    #[error("server hello can not be created without a compression method")]
    ErrCompressionMethodUnset,
    #[error("client+server cookie does not match")]
    ErrCookieMismatch,
    #[error("cookie must not be longer then 255 bytes")]
    ErrCookieTooLong,
    #[error("PSK Identity Hint provided but PSK is nil")]
    ErrIdentityNoPsk,
    #[error("no certificate provided")]
    ErrInvalidCertificate,
    #[error("cipher spec invalid")]
    ErrInvalidCipherSpec,
    #[error("invalid or unknown cipher suite")]
    ErrInvalidCipherSuite,
    #[error("unable to determine if ClientKeyExchange is a public key or PSK Identity")]
    ErrInvalidClientKeyExchange,
    #[error("invalid or unknown compression method")]
    ErrInvalidCompressionMethod,
    #[error("ECDSA signature contained zero or negative values")]
    ErrInvalidEcdsasignature,
    #[error("invalid or unknown elliptic curve type")]
    ErrInvalidEllipticCurveType,
    #[error("invalid extension type")]
    ErrInvalidExtensionType,
    #[error("invalid hash algorithm")]
    ErrInvalidHashAlgorithm,
    #[error("invalid named curve")]
    ErrInvalidNamedCurve,
    #[error("invalid private key type")]
    ErrInvalidPrivateKey,
    #[error("named curve and private key type does not match")]
    ErrNamedCurveAndPrivateKeyMismatch,
    #[error("invalid server name format")]
    ErrInvalidSniFormat,
    #[error("invalid signature algorithm")]
    ErrInvalidSignatureAlgorithm,
    #[error("expected and actual key signature do not match")]
    ErrKeySignatureMismatch,
    #[error("Conn can not be created with a nil nextConn")]
    ErrNilNextConn,
    #[error("connection can not be created, no CipherSuites satisfy this Config")]
    ErrNoAvailableCipherSuites,
    #[error("connection can not be created, no SignatureScheme satisfy this Config")]
    ErrNoAvailableSignatureSchemes,
    #[error("no certificates configured")]
    ErrNoCertificates,
    #[error("no config provided")]
    ErrNoConfigProvided,
    #[error("client requested zero or more elliptic curves that are not supported by the server")]
    ErrNoSupportedEllipticCurves,
    #[error("unsupported protocol version")]
    ErrUnsupportedProtocolVersion,
    #[error("Certificate and PSK provided")]
    ErrPskAndCertificate,
    #[error("PSK and PSK Identity Hint must both be set for client")]
    ErrPskAndIdentityMustBeSetForClient,
    #[error("SRTP support was requested but server did not respond with use_srtp extension")]
    ErrRequestedButNoSrtpExtension,
    #[error("Certificate is mandatory for server")]
    ErrServerMustHaveCertificate,
    #[error("client requested SRTP but we have no matching profiles")]
    ErrServerNoMatchingSrtpProfile,
    #[error(
        "server requires the Extended Master Secret extension, but the client does not support it"
    )]
    ErrServerRequiredButNoClientEms,
    #[error("expected and actual verify data does not match")]
    ErrVerifyDataMismatch,
    #[error("handshake message unset, unable to marshal")]
    ErrHandshakeMessageUnset,
    #[error("invalid flight number")]
    ErrInvalidFlight,
    #[error("unable to generate key signature, unimplemented")]
    ErrKeySignatureGenerateUnimplemented,
    #[error("unable to verify key signature, unimplemented")]
    ErrKeySignatureVerifyUnimplemented,
    #[error("data length and declared length do not match")]
    ErrLengthMismatch,
    #[error("buffer not long enough to contain nonce")]
    ErrNotEnoughRoomForNonce,
    #[error("feature has not been implemented yet")]
    ErrNotImplemented,
    #[error("sequence number overflow")]
    ErrSequenceNumberOverflow,
    #[error("unable to marshal fragmented handshakes")]
    ErrUnableToMarshalFragmented,
    #[error("invalid state machine transition")]
    ErrInvalidFsmTransition,
    #[error("ApplicationData with epoch of 0")]
    ErrApplicationDataEpochZero,
    #[error("unhandled contentType")]
    ErrUnhandledContextType,
    #[error("context canceled")]
    ErrContextCanceled,
    #[error("empty fragment")]
    ErrEmptyFragment,
    #[error("Alert is Fatal or Close Notify")]
    ErrAlertFatalOrClose,

    #[error("Other errors: {0}")]
    ErrOthers(String),

    #[error("SrtpError: {0}")]
    ErrSrtpError(#[from] srtp::error::Error),

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
