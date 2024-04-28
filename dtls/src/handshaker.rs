use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use log::*;

use crate::cipher_suite::*;
use crate::config::*;
use crate::conn::*;
use crate::content::*;
use crate::crypto::*;
use crate::error::*;
use crate::extension::extension_use_srtp::*;
use crate::signature_hash_algorithm::*;

use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::CertificateDer;
use rustls::server::danger::ClientCertVerifier;

//use std::io::BufWriter;

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

#[derive(Copy, Clone, PartialEq)]
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
    Arc<dyn (Fn(&[Vec<u8>], &[CertificateDer<'static>]) -> Result<()>) + Send + Sync>;

pub(crate) struct HandshakeConfig {
    pub(crate) local_psk_callback: Option<PskCallback>,
    pub(crate) local_psk_identity_hint: Option<Vec<u8>>,
    pub(crate) local_cipher_suites: Vec<CipherSuiteId>, // Available CipherSuites
    pub(crate) local_signature_schemes: Vec<SignatureHashAlgorithm>, // Available signature schemes
    pub(crate) extended_master_secret: ExtendedMasterSecretType, // Policy for the Extended Master Support extension
    pub(crate) local_srtp_protection_profiles: Vec<SrtpProtectionProfile>, // Available SRTPProtectionProfiles, if empty no SRTP support
    pub(crate) server_name: String,
    pub(crate) client_auth: ClientAuthType, // If we are a client should we request a client certificate
    pub(crate) local_certificates: Vec<Certificate>,
    pub(crate) name_to_certificate: HashMap<String, Certificate>,
    pub(crate) insecure_skip_verify: bool,
    pub(crate) insecure_verification: bool,
    pub(crate) verify_peer_certificate: Option<VerifyPeerCertificateFn>,
    pub(crate) server_cert_verifier: Arc<dyn ServerCertVerifier>,
    pub(crate) client_cert_verifier: Option<Arc<dyn ClientCertVerifier>>,
    pub(crate) retransmit_interval: tokio::time::Duration,
    pub(crate) initial_epoch: u16,
    //log           logging.LeveledLogger
    //mu sync.Mutex
}

pub fn gen_self_signed_root_cert() -> rustls::RootCertStore {
    let mut certs = rustls::RootCertStore::empty();
    certs
        .add(
            rcgen::generate_simple_self_signed(vec![])
                .unwrap()
                .cert
                .der()
                .to_owned(),
        )
        .unwrap();
    certs
}

impl Default for HandshakeConfig {
    fn default() -> Self {
        HandshakeConfig {
            local_psk_callback: None,
            local_psk_identity_hint: None,
            local_cipher_suites: vec![],
            local_signature_schemes: vec![],
            extended_master_secret: ExtendedMasterSecretType::Disable,
            local_srtp_protection_profiles: vec![],
            server_name: String::new(),
            client_auth: ClientAuthType::NoClientCert,
            local_certificates: vec![],
            name_to_certificate: HashMap::new(),
            insecure_skip_verify: false,
            insecure_verification: false,
            verify_peer_certificate: None,
            server_cert_verifier: rustls::client::WebPkiServerVerifier::builder(Arc::new(
                gen_self_signed_root_cert(),
            ))
            .build()
            .unwrap(),
            client_cert_verifier: None,
            retransmit_interval: tokio::time::Duration::from_secs(0),
            initial_epoch: 0,
        }
    }
}

impl HandshakeConfig {
    pub(crate) fn get_certificate(&self, server_name: &str) -> Result<Certificate> {
        //TODO
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
            return Err(Error::ErrNoCertificates);
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

pub(crate) fn srv_cli_str(is_client: bool) -> String {
    if is_client {
        return "client".to_owned();
    }
    "server".to_owned()
}

impl DTLSConn {
    pub(crate) async fn handshake(&mut self, mut state: HandshakeState) -> Result<()> {
        loop {
            trace!(
                "[handshake:{}] {}: {}",
                srv_cli_str(self.state.is_client),
                self.current_flight.to_string(),
                state.to_string()
            );

            if state == HandshakeState::Finished && !self.is_handshake_completed_successfully() {
                self.set_handshake_completed_successfully();
                self.handshake_done_tx.take(); // drop it by take
                return Ok(());
            }

            state = match state {
                HandshakeState::Preparing => self.prepare().await?,
                HandshakeState::Sending => self.send().await?,
                HandshakeState::Waiting => self.wait().await?,
                HandshakeState::Finished => self.finish().await?,
                _ => return Err(Error::ErrInvalidFsmTransition),
            };
        }
    }

    async fn prepare(&mut self) -> Result<HandshakeState> {
        self.flights = None;

        // Prepare flights
        self.retransmit = self.current_flight.has_retransmit();

        let result = self
            .current_flight
            .generate(&mut self.state, &self.cache, &self.cfg)
            .await;

        match result {
            Err((a, mut err)) => {
                if let Some(a) = a {
                    let alert_err = self.notify(a.alert_level, a.alert_description).await;

                    if let Err(alert_err) = alert_err {
                        if err.is_some() {
                            err = Some(alert_err);
                        }
                    }
                }
                if let Some(err) = err {
                    return Err(err);
                }
            }
            Ok(pkts) => {
                /*if !pkts.is_empty() {
                    let mut s = vec![];
                    {
                        let mut writer = BufWriter::<&mut Vec<u8>>::new(s.as_mut());
                        pkts[0].record.content.marshal(&mut writer)?;
                    }
                    trace!(
                        "[handshake:{}] {}: {:?}",
                        srv_cli_str(self.state.is_client),
                        self.current_flight.to_string(),
                        s,
                    );
                }*/
                self.flights = Some(pkts)
            }
        };

        let epoch = self.cfg.initial_epoch;
        let mut next_epoch = epoch;
        if let Some(pkts) = &mut self.flights {
            for p in pkts {
                p.record.record_layer_header.epoch += epoch;
                if p.record.record_layer_header.epoch > next_epoch {
                    next_epoch = p.record.record_layer_header.epoch;
                }
                if let Content::Handshake(h) = &mut p.record.content {
                    h.handshake_header.message_sequence = self.state.handshake_send_sequence as u16;
                    self.state.handshake_send_sequence += 1;
                }
            }
        }
        if epoch != next_epoch {
            trace!(
                "[handshake:{}] -> changeCipherSpec (epoch: {})",
                srv_cli_str(self.state.is_client),
                next_epoch
            );
            self.set_local_epoch(next_epoch);
        }

        Ok(HandshakeState::Sending)
    }
    async fn send(&mut self) -> Result<HandshakeState> {
        // Send flights
        if let Some(pkts) = self.flights.clone() {
            self.write_packets(pkts).await?;
        }

        if self.current_flight.is_last_send_flight() {
            Ok(HandshakeState::Finished)
        } else {
            Ok(HandshakeState::Waiting)
        }
    }
    async fn wait(&mut self) -> Result<HandshakeState> {
        let retransmit_timer = tokio::time::sleep(self.cfg.retransmit_interval);
        tokio::pin!(retransmit_timer);

        loop {
            tokio::select! {
                 done = self.handshake_rx.recv() =>{
                    if done.is_none() {
                        trace!("[handshake:{}] {} handshake_tx is dropped", srv_cli_str(self.state.is_client), self.current_flight.to_string());
                        return Err(Error::ErrAlertFatalOrClose);
                    }

                    //trace!("[handshake:{}] {} received handshake_rx", srv_cli_str(self.state.is_client), self.current_flight.to_string());
                    let result = self.current_flight.parse(&mut self.handle_queue_tx, &mut self.state, &self.cache, &self.cfg).await;
                    drop(done);
                    match result {
                        Err((alert, mut err)) => {
                            trace!("[handshake:{}] {} result alert:{:?}, err:{:?}",
                                    srv_cli_str(self.state.is_client),
                                    self.current_flight.to_string(),
                                    alert,
                                    err);

                            if let Some(alert) = alert {
                                let alert_err = self.notify(alert.alert_level, alert.alert_description).await;

                                if let Err(alert_err) = alert_err {
                                    if err.is_some() {
                                        err = Some(alert_err);
                                    }
                                }
                            }
                            if let Some(err) = err {
                                return Err(err);
                            }
                        }
                        Ok(next_flight) => {
                            trace!("[handshake:{}] {} -> {}", srv_cli_str(self.state.is_client), self.current_flight.to_string(), next_flight.to_string());
                            if next_flight.is_last_recv_flight() && self.current_flight.to_string() == next_flight.to_string() {
                                return Ok(HandshakeState::Finished);
                            }
                            self.current_flight = next_flight;
                            return Ok(HandshakeState::Preparing);
                        }
                    };
                }

                _ = retransmit_timer.as_mut() =>{
                    trace!("[handshake:{}] {} retransmit_timer", srv_cli_str(self.state.is_client), self.current_flight.to_string());

                    if !self.retransmit {
                        return Ok(HandshakeState::Waiting);
                    }
                    return Ok(HandshakeState::Sending);
                }

                /*_ = self.done_rx.recv() => {
                    return Err(Error::new("done_rx recv".to_owned()));
                }*/
            }
        }
    }
    async fn finish(&mut self) -> Result<HandshakeState> {
        let retransmit_timer = tokio::time::sleep(self.cfg.retransmit_interval);

        tokio::select! {
            done = self.handshake_rx.recv() =>{
                if done.is_none() {
                    trace!("[handshake:{}] {} handshake_tx is dropped", srv_cli_str(self.state.is_client), self.current_flight.to_string());
                    return Err(Error::ErrAlertFatalOrClose);
                }
                let result = self.current_flight.parse(&mut self.handle_queue_tx, &mut self.state, &self.cache, &self.cfg).await;
                drop(done);
                match result {
                    Err((alert, mut err)) => {
                        if let Some(alert) = alert {
                            let alert_err = self.notify(alert.alert_level, alert.alert_description).await;
                            if let Err(alert_err) = alert_err {
                                if err.is_some() {
                                    err = Some(alert_err);
                                }
                            }
                        }
                        if let Some(err) = err {
                            return Err(err);
                        }
                    }
                    Ok(_) => {
                        retransmit_timer.await;
                        // Retransmit last flight
                        return Ok(HandshakeState::Sending);
                    }
                };
            }

            /*_ = self.done_rx.recv() => {
                return Err(Error::new("done_rx recv".to_owned()));
            }*/
        }

        Ok(HandshakeState::Finished)
    }
}
