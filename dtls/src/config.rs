use crate::cipher_suite::*;
use crate::errors::*;
use crate::extension::extension_use_srtp::SRTPProtectionProfile;

use std::time::Duration;

use util::Error;

// Config is used to configure a DTLS client or server.
// After a Config is passed to a DTLS function it must not be modified.
pub struct Config {
    // Certificates contains certificate chain to present to the other side of the connection.
    // Server MUST set this if psk is non-nil
    // client SHOULD sets this so CertificateRequests can be handled if psk is non-nil
    //TODO: Certificates []tls.Certificate

    // cipher_suites is a list of supported cipher suites.
    // If cipher_suites is nil, a default list is used
    cipher_suites: Vec<CipherSuiteID>,

    // SignatureSchemes contains the signature and hash schemes that the peer requests to verify.
    //TODO: SignatureSchemes: []tls.SignatureScheme

    // srtpprotection_profiles are the supported protection profiles
    // Clients will send this via use_srtp and assert that the server properly responds
    // Servers will assert that clients send one of these profiles and will respond as needed
    srtp_protection_profiles: Vec<SRTPProtectionProfile>,

    // client_auth determines the server's policy for
    // TLS Client Authentication. The default is NoClientCert.
    client_auth: ClientAuthType,

    // RequireExtendedMasterSecret determines if the "Extended Master Secret" extension
    // should be disabled, requested, or required (default requested).
    extended_master_secret: ExtendedMasterSecretType,

    // flight_interval controls how often we send outbound handshake messages
    // defaults to time.Second
    flight_interval: Duration,

    // psk sets the pre-shared key used by this DTLS connection
    // If psk is non-nil only psk cipher_suites will be used
    psk: Option<PSKCallback>,
    psk_identity_hint: Vec<u8>,

    // insecure_skip_verify controls whether a client verifies the
    // server's certificate chain and host name.
    // If insecure_skip_verify is true, TLS accepts any certificate
    // presented by the server and any host name in that certificate.
    // In this mode, TLS is susceptible to man-in-the-middle attacks.
    // This should be used only for testing.
    insecure_skip_verify: bool,

    // insecure_hashes allows the use of hashing algorithms that are known
    // to be vulnerable.
    insecure_hashes: bool,

    // VerifyPeerCertificate, if not nil, is called after normal
    // certificate verification by either a client or server. It
    // receives the certificate provided by the peer and also a flag
    // that tells if normal verification has succeedded. If it returns a
    // non-nil error, the handshake is aborted and that error results.
    //
    // If normal verification fails then the handshake will abort before
    // considering this callback. If normal verification is disabled by
    // setting insecure_skip_verify, or (for a server) when client_auth is
    // RequestClientCert or RequireAnyClientCert, then this callback will
    // be considered but the verifiedChains will always be nil.
    //TODO: VerifyPeerCertificate func(rawCerts [][]byte, verifiedChains [][]*x509.Certificate) error

    // RootCAs defines the set of root certificate authorities
    // that one peer uses when verifying the other peer's certificates.
    // If RootCAs is nil, TLS uses the host's root CA set.
    //TODO: RootCAs *x509.CertPool

    // ClientCAs defines the set of root certificate authorities
    // that servers use if required to verify a client certificate
    // by the policy in client_auth.
    //TODO: ClientCAs *x509.CertPool

    // server_name is used to verify the hostname on the returned
    // certificates unless insecure_skip_verify is given.
    server_name: String,

    //TODO: LoggerFactory logging.LoggerFactory

    // ConnectContextMaker is a function to make a context used in Dial(),
    // Client(), Server(), and Accept(). If nil, the default ConnectContextMaker
    // is used. It can be implemented as following.
    //
    // 	func ConnectContextMaker() (context.Context, func()) {
    // 		return context.WithTimeout(context.Background(), 30*time.Second)
    // 	}
    //TODO: ConnectContextMaker func() (context.Context, func())

    // mtu is the length at which handshake messages will be fragmented to
    // fit within the maximum transmission unit (default is 1200 bytes)
    mtu: usize,

    // replay_protection_window is the size of the replay attack protection window.
    // Duplication of the sequence number is checked in this window size.
    // Packet with sequence number older than this value compared to the latest
    // accepted packet will be discarded. (default is 64)
    replay_protection_window: usize,
}

const DEFAULT_MTU: usize = 1200; // bytes

// PSKCallback is called once we have the remote's psk_identity_hint.
// If the remote provided none it will be nil
type PSKCallback = fn(&[u8]) -> Result<Vec<u8>, Error>;

// ClientAuthType declares the policy the server will follow for
// TLS Client Authentication.
enum ClientAuthType {
    NoClientCert = 0,
    RequestClientCert = 1,
    RequireAnyClientCert = 2,
    VerifyClientCertIfGiven = 3,
    RequireAndVerifyClientCert = 4,
}

// ExtendedMasterSecretType declares the policy the client and server
// will follow for the Extended Master Secret extension
enum ExtendedMasterSecretType {
    RequestExtendedMasterSecret = 0,
    RequireExtendedMasterSecret = 1,
    DisableExtendedMasterSecret = 2,
}

pub(crate) fn validate_config(config: &Config) -> Result<(), Error> {
    //TODO: if config.Certificates.len() > 0 && config.psk != nil:
    //	return ErrPSKAndCertificate

    if config.psk_identity_hint.len() != 0 && config.psk.is_none() {
        return Err(ERR_IDENTITY_NO_PSK.clone());
    }

    /*TODO: for _, cert := range config.Certificates {
        if cert.Certificate == nil {
            return errInvalidCertificate
        }
        if cert.PrivateKey != nil {
            switch cert.PrivateKey.(type) {
            case ed25519.PrivateKey:
            case *ecdsa.PrivateKey:
            default:
                return errInvalidPrivateKey
            }
        }
    }*/

    parse_cipher_suites(
        &config.cipher_suites,
        config.psk.is_none(),
        config.psk.is_some(),
    )?;

    Ok(())
}
