use util::Error;

lazy_static! {
    pub static ref ERR_CONN_CLOSED: Error = Error::new("conn is closed".to_owned());
    pub static ref ERR_DEADLINE_EXCEEDED: Error = Error::new("read/write timeout".to_owned());
    pub static ref ERR_BUFFER_TOO_SMALL: Error = Error::new("buffer is too small".to_owned());
    pub static ref ERR_CONTEXT_UNSUPPORTED: Error =
        Error::new("context is not supported for export_keying_material".to_owned());
    pub static ref ERR_DTLSPACKET_INVALID_LENGTH: Error =
        Error::new("packet is too short".to_owned());
    pub static ref ERR_HANDSHAKE_IN_PROGRESS: Error =
        Error::new("handshake is in progress".to_owned());
    pub static ref ERR_INVALID_CONTENT_TYPE: Error = Error::new("invalid content type".to_owned());
    pub static ref ERR_INVALID_MAC: Error = Error::new("invalid mac".to_owned());
    pub static ref ERR_INVALID_PACKET_LENGTH: Error =
        Error::new("packet length and declared length do not match".to_owned());
    pub static ref ERR_RESERVED_EXPORT_KEYING_MATERIAL: Error =
        Error::new("export_keying_material can not be used with a reserved label".to_owned());
    pub static ref ERR_CERTIFICATE_VERIFY_NO_CERTIFICATE: Error = Error::new(
        "client sent certificate verify but we have no certificate to verify".to_owned()
    );
    pub static ref ERR_CIPHER_SUITE_NO_INTERSECTION: Error =
        Error::new("client+server do not support any shared cipher suites".to_owned());
    pub static ref ERR_CIPHER_SUITE_UNSET: Error =
        Error::new("server hello can not be created without a cipher suite".to_owned());
    pub static ref ERR_CLIENT_CERTIFICATE_NOT_VERIFIED: Error =
        Error::new("client sent certificate but did not verify it".to_owned());
    pub static ref ERR_CLIENT_CERTIFICATE_REQUIRED: Error =
        Error::new("server required client verification, but got none".to_owned());
    pub static ref ERR_CLIENT_NO_MATCHING_SRTP_PROFILE: Error =
        Error::new("server responded with SRTP Profile we do not support".to_owned());
    pub static ref ERR_CLIENT_REQUIRED_BUT_NO_SERVER_EMS: Error = Error::new(
        "client required Extended Master Secret extension, but server does not support it"
            .to_owned()
    );
    pub static ref ERR_COMPRESSION_METHOD_UNSET: Error =
        Error::new("server hello can not be created without a compression method".to_owned());
    pub static ref ERR_COOKIE_MISMATCH: Error =
        Error::new("client+server cookie does not match".to_owned());
    pub static ref ERR_COOKIE_TOO_LONG: Error =
        Error::new("cookie must not be longer then 255 bytes".to_owned());
    pub static ref ERR_IDENTITY_NO_PSK: Error =
        Error::new("PSK Identity Hint provided but PSK is nil".to_owned());
    pub static ref ERR_INVALID_CERTIFICATE: Error =
        Error::new("no certificate provided".to_owned());
    pub static ref ERR_INVALID_CIPHER_SPEC: Error = Error::new("cipher spec invalid".to_owned());
    pub static ref ERR_INVALID_CIPHER_SUITE: Error =
        Error::new("invalid or unknown cipher suite".to_owned());
    pub static ref ERR_INVALID_CLIENT_KEY_EXCHANGE: Error = Error::new(
        "unable to determine if ClientKeyExchange is a public key or PSK Identity".to_owned()
    );
    pub static ref ERR_INVALID_COMPRESSION_METHOD: Error =
        Error::new("invalid or unknown compression method".to_owned());
    pub static ref ERR_INVALID_ECDSASIGNATURE: Error =
        Error::new("ECDSA signature contained zero or negative values".to_owned());
    pub static ref ERR_INVALID_ELLIPTIC_CURVE_TYPE: Error =
        Error::new("invalid or unknown elliptic curve type".to_owned());
    pub static ref ERR_INVALID_EXTENSION_TYPE: Error =
        Error::new("invalid extension type".to_owned());
    pub static ref ERR_INVALID_HASH_ALGORITHM: Error =
        Error::new("invalid hash algorithm".to_owned());
    pub static ref ERR_INVALID_NAMED_CURVE: Error = Error::new("invalid named curve".to_owned());
    pub static ref ERR_INVALID_PRIVATE_KEY: Error =
        Error::new("invalid private key type".to_owned());
    pub static ref ERR_NAMED_CURVE_AND_PRIVATE_KEY_MISMATCH: Error =
        Error::new("named curve and private key type does not match".to_owned());
    pub static ref ERR_INVALID_SNI_FORMAT: Error =
        Error::new("invalid server name format".to_owned());
    pub static ref ERR_INVALID_SIGNATURE_ALGORITHM: Error =
        Error::new("invalid signature algorithm".to_owned());
    pub static ref ERR_KEY_SIGNATURE_MISMATCH: Error =
        Error::new("expected and actual key signature do not match".to_owned());
    pub static ref ERR_NIL_NEXT_CONN: Error =
        Error::new("Conn can not be created with a nil nextConn".to_owned());
    pub static ref ERR_NO_AVAILABLE_CIPHER_SUITES: Error =
        Error::new("connection can not be created, no CipherSuites satisfy this Config".to_owned());
    pub static ref ERR_NO_AVAILABLE_SIGNATURE_SCHEMES: Error = Error::new(
        "connection can not be created, no SignatureScheme satisfy this Config".to_owned()
    );
    pub static ref ERR_NO_CERTIFICATES: Error = Error::new("no certificates configured".to_owned());
    pub static ref ERR_NO_CONFIG_PROVIDED: Error = Error::new("no config provided".to_owned());
    pub static ref ERR_NO_SUPPORTED_ELLIPTIC_CURVES: Error = Error::new(
        "client requested zero or more elliptic curves that are not supported by the server"
            .to_owned()
    );
    pub static ref ERR_UNSUPPORTED_PROTOCOL_VERSION: Error =
        Error::new("unsupported protocol version".to_owned());
    pub static ref ERR_PSK_AND_CERTIFICATE: Error =
        Error::new("Certificate and PSK provided".to_owned());
    pub static ref ERR_PSK_AND_IDENTITY_MUST_BE_SET_FOR_CLIENT: Error =
        Error::new("PSK and PSK Identity Hint must both be set for client".to_owned());
    pub static ref ERR_REQUESTED_BUT_NO_SRTP_EXTENSION: Error = Error::new(
        "SRTP support was requested but server did not respond with use_srtp extension".to_owned()
    );
    pub static ref ERR_SERVER_MUST_HAVE_CERTIFICATE: Error =
        Error::new("Certificate is mandatory for server".to_owned());
    pub static ref ERR_SERVER_NO_MATCHING_SRTP_PROFILE: Error =
        Error::new("client requested SRTP but we have no matching profiles".to_owned());
    pub static ref ERR_SERVER_REQUIRED_BUT_NO_CLIENT_EMS: Error = Error::new(
        "server requires the Extended Master Secret extension, but the client does not support it"
            .to_owned()
    );
    pub static ref ERR_VERIFY_DATA_MISMATCH: Error =
        Error::new("expected and actual verify data does not match".to_owned());
    pub static ref ERR_HANDSHAKE_MESSAGE_UNSET: Error =
        Error::new("handshake message unset, unable to marshal".to_owned());
    pub static ref ERR_INVALID_FLIGHT: Error = Error::new("invalid flight number".to_owned());
    pub static ref ERR_KEY_SIGNATURE_GENERATE_UNIMPLEMENTED: Error =
        Error::new("unable to generate key signature, unimplemented".to_owned());
    pub static ref ERR_KEY_SIGNATURE_VERIFY_UNIMPLEMENTED: Error =
        Error::new("unable to verify key signature, unimplemented".to_owned());
    pub static ref ERR_LENGTH_MISMATCH: Error =
        Error::new("data length and declared length do not match".to_owned());
    pub static ref ERR_NOT_ENOUGH_ROOM_FOR_NONCE: Error =
        Error::new("buffer not long enough to contain nonce".to_owned());
    pub static ref ERR_NOT_IMPLEMENTED: Error =
        Error::new("feature has not been implemented yet".to_owned());
    pub static ref ERR_SEQUENCE_NUMBER_OVERFLOW: Error =
        Error::new("sequence number overflow".to_owned());
    pub static ref ERR_UNABLE_TO_MARSHAL_FRAGMENTED: Error =
        Error::new("unable to marshal fragmented handshakes".to_owned());
    pub static ref ERR_INVALID_FSM_TRANSITION: Error =
        Error::new("invalid state machine transition".to_owned());
    pub static ref ERR_APPLICATION_DATA_EPOCH_ZERO: Error =
        Error::new("ApplicationData with epoch of 0".to_owned());
    pub static ref ERR_UNHANDLED_CONTEXT_TYPE: Error =
        Error::new("unhandled contentType".to_owned());
    pub static ref ERR_CONTEXT_CANCELED: Error = Error::new("context canceled".to_owned());
    pub static ref ERR_EMPTY_FRAGMENT: Error = Error::new("empty fragment".to_owned());
    pub static ref ERR_ALERT_FATAL_OR_CLOSE: Error =
        Error::new("Alert is Fatal or Close Notify".to_owned());
}
