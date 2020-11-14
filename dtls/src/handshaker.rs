use crate::cipher_suite::*;
use crate::config::*;
use crate::crypto::*;
use crate::extension::extension_use_srtp::*;
use crate::flight::*;
use crate::signature_hash_algorithm::*;

use util::Error;

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

// [RFC6347 Section-4.2.4]
//                      +-----------+
//                +---> | PREPARING | <--------------------+
//                |     +-----------+                      |
//                |           |                            |
//                |           | Buffer next flight         |
//                |           |                            |
//                |          \|/                           |
//                |     +-----------+                      |
//                |     |  SENDING  |<------------------+  | Send
//                |     +-----------+                   |  | HelloRequest
//        Receive |           |                         |  |
//           next |           | Send flight             |  | or
//         flight |  +--------+                         |  |
//                |  |        | Set retransmit timer    |  | Receive
//                |  |       \|/                        |  | HelloRequest
//                |  |  +-----------+                   |  | Send
//                +--)--|  WAITING  |-------------------+  | ClientHello
//                |  |  +-----------+   Timer expires   |  |
//                |  |         |                        |  |
//                |  |         +------------------------+  |
//        Receive |  | Send           Read retransmit      |
//           last |  | last                                |
//         flight |  | flight                              |
//                |  |                                     |
//               \|/\|/                                    |
//            +-----------+                                |
//            | FINISHED  | -------------------------------+
//            +-----------+
//                 |  /|\
//                 |   |
//                 +---+
//              Read retransmit
//           Retransmit last flight

pub(crate) enum HandshakeState {
    Errored,
    Preparing,
    Sending,
    Waiting,
    Finished,
}

impl fmt::Display for HandshakeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            HandshakeState::Errored => write!(f, "Errored"),
            HandshakeState::Preparing => write!(f, "Preparing"),
            HandshakeState::Sending => write!(f, "Sending"),
            HandshakeState::Waiting => write!(f, "Waiting"),
            HandshakeState::Finished => write!(f, "Finished"),
        }
    }
}

pub(crate) type VerifyPeerCertificateFn =
    fn(rawCerts: &[u8], verifiedChains: &Certificate) -> Result<(), Error>;

pub(crate) type OnFlightStateFn = fn(f: Flight, hs: HandshakeState);

pub(crate) struct HandshakeConfig {
    pub(crate) local_psk_callback: Option<PSKCallback>,
    pub(crate) local_psk_identity_hint: Vec<u8>,
    pub(crate) local_cipher_suites: Vec<CipherSuiteID>, // Available CipherSuites
    pub(crate) local_signature_schemes: Vec<SignatureHashAlgorithm>, // Available signature schemes
    pub(crate) extended_master_secret: ExtendedMasterSecretType, // Policy for the Extended Master Support extension
    pub(crate) local_srtp_protection_profiles: Vec<SRTPProtectionProfile>, // Available SRTPProtectionProfiles, if empty no SRTP support
    pub(crate) server_name: String,
    pub(crate) client_auth: ClientAuthType, // If we are a client should we request a client certificate
    pub(crate) local_certificates: Vec<Certificate>,
    pub(crate) name_to_certificate: HashMap<String, Certificate>,
    pub(crate) insecure_skip_verify: bool,
    pub(crate) verify_peer_certificate: VerifyPeerCertificateFn,
    //rootCAs                     *x509.CertPool
    //clientCAs                   *x509.CertPool
    pub(crate) retransmit_interval: Duration,

    pub(crate) on_flight_state: OnFlightStateFn,
    //log           logging.LeveledLogger
    pub(crate) initial_epoch: u16,
    //mu sync.Mutex
}
