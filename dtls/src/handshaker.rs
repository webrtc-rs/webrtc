use crate::cipher_suite::*;
use crate::config::*;
use crate::crypto::*;
use crate::errors::*;
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
    fn(rawCerts: &[u8], verifiedChains: &[x509_parser::X509Certificate<'_>]) -> Result<(), Error>;

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
    pub(crate) verify_peer_certificate: Option<VerifyPeerCertificateFn>,
    //rootCAs                     *x509.CertPool
    //clientCAs                   *x509.CertPool
    pub(crate) retransmit_interval: Duration,

    pub(crate) on_flight_state: OnFlightStateFn,
    //log           logging.LeveledLogger
    pub(crate) initial_epoch: u16,
    //mu sync.Mutex
}

impl HandshakeConfig {
    pub(crate) fn get_certificate(&self, server_name: &str) -> Result<Certificate, Error> {
        //TODO: add mutex
        //c.mu.Lock()
        //defer c.mu.Unlock()

        /*if self.name_to_certificate.is_empty() {
            let mut name_to_certificate = HashMap::new();
            for cert in &self.local_certificates {
                if let Ok((_rem, x509_cert)) = x509_parser::parse_x509_der(&cert.certificate) {
                    if let Some(a) = x509_cert.tbs_certificate.subject.iter_common_name().next() {
                        let common_name = match a.attr_value.as_str() {
                            Ok(cn) => cn.to_lowercase(),
                            Err(err) => return Err(Error::new(err.to_string())),
                        };
                        name_to_certificate.insert(common_name, cert.clone());
                    }
                    if let Some((_, sans)) = x509_cert.tbs_certificate.subject_alternative_name() {
                        for gn in &sans.general_names {
                            match gn {
                                x509_parser::extensions::GeneralName::DNSName(san) => {
                                    let san = san.to_lowercase();
                                    name_to_certificate.insert(san, cert.clone());
                                }
                                _ => {}
                            }
                        }
                    }
                } else {
                    continue;
                }
            }
            self.name_to_certificate = name_to_certificate;
        }*/

        if self.local_certificates.is_empty() {
            return Err(ERR_NO_CERTIFICATES.clone());
        }

        if self.local_certificates.len() == 1 {
            // There's only one choice, so no point doing any work.
            return Ok(self.local_certificates[0].clone());
        }

        if server_name.is_empty() {
            return Ok(self.local_certificates[0].clone());
        }

        let lower = server_name.to_lowercase();
        let name = lower.trim_end_matches('.');

        if let Some(cert) = self.name_to_certificate.get(name) {
            return Ok(cert.clone());
        }

        // try replacing labels in the name with wildcards until we get a
        // match.
        let mut labels: Vec<&str> = name.split_terminator('.').collect();
        for i in 0..labels.len() {
            labels[i] = "*";
            let candidate = labels.join(".");
            if let Some(cert) = self.name_to_certificate.get(&candidate) {
                return Ok(cert.clone());
            }
        }

        // If nothing matches, return the first certificate.
        Ok(self.local_certificates[0].clone())
    }
}
