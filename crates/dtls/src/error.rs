use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
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

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        err.downcast_ref::<Self>().map_or(false, |e| e == self)
    }
}
