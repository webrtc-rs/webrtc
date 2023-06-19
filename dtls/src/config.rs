use std::sync::Arc;

use tokio::time::Duration;

use crate::cipher_suite::*;
use crate::crypto::*;
use crate::error::*;
use crate::extension::extension_use_srtp::SrtpProtectionProfile;
use crate::handshaker::VerifyPeerCertificateFn;
use crate::signature_hash_algorithm::SignatureScheme;

/// Config is used to configure a DTLS client or server.
/// After a Config is passed to a DTLS function it must not be modified.
#[derive(Clone)]
pub struct Config {
    /// certificates contains certificate chain to present to the other side of the connection.
    /// Server MUST set this if psk is non-nil
    /// client SHOULD sets this so CertificateRequests can be handled if psk is non-nil
    pub certificates: Vec<Certificate>,

    /// cipher_suites is a list of supported cipher suites.
    /// If cipher_suites is nil, a default list is used
    pub cipher_suites: Vec<CipherSuiteId>,

    /// signature_schemes contains the signature and hash schemes that the peer requests to verify.
    pub signature_schemes: Vec<SignatureScheme>,

    /// srtp_protection_profiles are the supported protection profiles
    /// Clients will send this via use_srtp and assert that the server properly responds
    /// Servers will assert that clients send one of these profiles and will respond as needed
    pub srtp_protection_profiles: Vec<SrtpProtectionProfile>,

    /// client_auth determines the server's policy for
    /// TLS Client Authentication. The default is NoClientCert.
    pub client_auth: ClientAuthType,

    /// extended_master_secret determines if the "Extended Master Secret" extension
    /// should be disabled, requested, or required (default requested).
    pub extended_master_secret: ExtendedMasterSecretType,

    /// flight_interval controls how often we send outbound handshake messages
    /// defaults to time.Second
    pub flight_interval: Duration,

    /// psk sets the pre-shared key used by this DTLS connection
    /// If psk is non-nil only psk cipher_suites will be used
    pub psk: Option<PskCallback>,
    pub psk_identity_hint: Option<Vec<u8>>,

    /// insecure_skip_verify controls whether a client verifies the
    /// server's certificate chain and host name.
    /// If insecure_skip_verify is true, TLS accepts any certificate
    /// presented by the server and any host name in that certificate.
    /// In this mode, TLS is susceptible to man-in-the-middle attacks.
    /// This should be used only for testing.
    pub insecure_skip_verify: bool,

    /// insecure_hashes allows the use of hashing algorithms that are known
    /// to be vulnerable.
    pub insecure_hashes: bool,

    /// insecure_verification allows the use of verification algorithms that are
    /// known to be vulnerable or deprecated
    pub insecure_verification: bool,
    /// VerifyPeerCertificate, if not nil, is called after normal
    /// certificate verification by either a client or server. It
    /// receives the certificate provided by the peer and also a flag
    /// that tells if normal verification has succeeded. If it returns a
    /// non-nil error, the handshake is aborted and that error results.
    ///
    /// If normal verification fails then the handshake will abort before
    /// considering this callback. If normal verification is disabled by
    /// setting insecure_skip_verify, or (for a server) when client_auth is
    /// RequestClientCert or RequireAnyClientCert, then this callback will
    /// be considered but the verifiedChains will always be nil.
    pub verify_peer_certificate: Option<VerifyPeerCertificateFn>,

    /// roots_cas defines the set of root certificate authorities
    /// that one peer uses when verifying the other peer's certificates.
    /// If RootCAs is nil, TLS uses the host's root CA set.
    /// Used by Client to verify server's certificate
    pub roots_cas: rustls::RootCertStore,

    /// client_cas defines the set of root certificate authorities
    /// that servers use if required to verify a client certificate
    /// by the policy in client_auth.
    /// Used by Server to verify client's certificate
    pub client_cas: rustls::RootCertStore,

    /// server_name is used to verify the hostname on the returned
    /// certificates unless insecure_skip_verify is given.
    pub server_name: String,

    /// mtu is the length at which handshake messages will be fragmented to
    /// fit within the maximum transmission unit (default is 1200 bytes)
    pub mtu: usize,

    /// replay_protection_window is the size of the replay attack protection window.
    /// Duplication of the sequence number is checked in this window size.
    /// Packet with sequence number older than this value compared to the latest
    /// accepted packet will be discarded. (default is 64)
    pub replay_protection_window: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            certificates: vec![],
            cipher_suites: vec![],
            signature_schemes: vec![],
            srtp_protection_profiles: vec![],
            client_auth: ClientAuthType::default(),
            extended_master_secret: ExtendedMasterSecretType::default(),
            flight_interval: Duration::default(),
            psk: None,
            psk_identity_hint: None,
            insecure_skip_verify: false,
            insecure_hashes: false,
            insecure_verification: false,
            verify_peer_certificate: None,
            roots_cas: rustls::RootCertStore::empty(),
            client_cas: rustls::RootCertStore::empty(),
            server_name: String::default(),
            mtu: 0,
            replay_protection_window: 0,
        }
    }
}

pub(crate) const DEFAULT_MTU: usize = 1200; // bytes

// PSKCallback is called once we have the remote's psk_identity_hint.
// If the remote provided none it will be nil
pub(crate) type PskCallback = Arc<dyn (Fn(&[u8]) -> Result<Vec<u8>>) + Send + Sync>;

// ClientAuthType declares the policy the server will follow for
// TLS Client Authentication.
#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub enum ClientAuthType {
    #[default]
    NoClientCert = 0,
    RequestClientCert = 1,
    RequireAnyClientCert = 2,
    VerifyClientCertIfGiven = 3,
    RequireAndVerifyClientCert = 4,
}

// ExtendedMasterSecretType declares the policy the client and server
// will follow for the Extended Master Secret extension
#[derive(Default, PartialEq, Eq, Copy, Clone)]
pub enum ExtendedMasterSecretType {
    #[default]
    Request = 0,
    Require = 1,
    Disable = 2,
}

pub(crate) fn validate_config(is_client: bool, config: &Config) -> Result<()> {
    if is_client && config.psk.is_some() && config.psk_identity_hint.is_none() {
        return Err(Error::ErrPskAndIdentityMustBeSetForClient);
    }

    if !is_client && config.psk.is_none() && config.certificates.is_empty() {
        return Err(Error::ErrServerMustHaveCertificate);
    }

    if !config.certificates.is_empty() && config.psk.is_some() {
        return Err(Error::ErrPskAndCertificate);
    }

    if config.psk_identity_hint.is_some() && config.psk.is_none() {
        return Err(Error::ErrIdentityNoPsk);
    }

    for cert in &config.certificates {
        match cert.private_key.kind {
            CryptoPrivateKeyKind::Ed25519(_) => {}
            CryptoPrivateKeyKind::Ecdsa256(_) => {}
            _ => return Err(Error::ErrInvalidPrivateKey),
        }
    }

    parse_cipher_suites(
        &config.cipher_suites,
        config.psk.is_none(),
        config.psk.is_some(),
    )?;

    Ok(())
}
