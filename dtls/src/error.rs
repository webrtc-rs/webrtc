use std::io;
use std::string::FromUtf8Error;

use thiserror::Error;
use tokio::sync::mpsc::error::SendError as MpscSendError;
use util::KeyingMaterialExporterError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
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

    #[error(
        "Fragment buffer overflow. New size {new_size} is greater than specified max {max_size}"
    )]
    ErrFragmentBufferOverflow { new_size: usize, max_size: usize },

    #[error("{0}")]
    Io(#[source] IoError),
    #[error("{0}")]
    Util(#[from] util::Error),
    #[error("utf8: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("{0}")]
    Sec1(#[source] sec1::Error),
    #[error("{0}")]
    Aes(#[from] aes::cipher::InvalidLength),
    #[error("{0}")]
    P256(#[source] P256Error),
    #[error("{0}")]
    RcGen(#[from] rcgen::Error),
    #[error("mpsc send: {0}")]
    MpscSend(String),
    #[error("keying material: {0}")]
    KeyingMaterial(#[from] KeyingMaterialExporterError),

    /// Error parsing a given PEM string.
    #[error("invalid PEM: {0}")]
    InvalidPEM(String),

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    Other(String),
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

impl From<sec1::Error> for Error {
    fn from(e: sec1::Error) -> Self {
        Error::Sec1(e)
    }
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct P256Error(#[source] p256::elliptic_curve::Error);

impl PartialEq for P256Error {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

impl From<p256::elliptic_curve::Error> for Error {
    fn from(e: p256::elliptic_curve::Error) -> Self {
        Error::P256(P256Error(e))
    }
}

// Because Tokio SendError is parameterized, we sadly lose the backtrace.
impl<T> From<MpscSendError<T>> for Error {
    fn from(e: MpscSendError<T>) -> Self {
        Error::MpscSend(e.to_string())
    }
}
