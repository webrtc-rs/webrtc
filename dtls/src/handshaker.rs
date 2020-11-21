use crate::cipher_suite::*;
use crate::config::*;
use crate::conn::*;
use crate::content::*;
use crate::crypto::*;
use crate::errors::*;
use crate::extension::extension_use_srtp::*;
use crate::flight::*;
use crate::handshake::handshake_cache::*;
use crate::signature_hash_algorithm::*;
use crate::state::*;

use log::*;

use util::Error;

use std::collections::HashMap;
use std::fmt;

use tokio::sync::mpsc;

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

#[derive(Copy, Clone)]
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

pub(crate) type OnFlightStateFn = fn(f: &Box<dyn Flight>, hs: HandshakeState);

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
    pub(crate) retransmit_interval: tokio::time::Duration,

    pub(crate) on_flight_state: Option<OnFlightStateFn>,
    //log           logging.LeveledLogger
    pub(crate) initial_epoch: u16,
    //mu sync.Mutex
}

impl Default for HandshakeConfig {
    fn default() -> Self {
        HandshakeConfig {
            local_psk_callback: None,
            local_psk_identity_hint: vec![],
            local_cipher_suites: vec![],
            local_signature_schemes: vec![],
            extended_master_secret: ExtendedMasterSecretType::Disable,
            local_srtp_protection_profiles: vec![],
            server_name: String::new(),
            client_auth: ClientAuthType::NoClientCert,
            local_certificates: vec![],
            name_to_certificate: HashMap::new(),
            insecure_skip_verify: false,
            verify_peer_certificate: None,
            retransmit_interval: tokio::time::Duration::from_secs(0),
            on_flight_state: None,
            initial_epoch: 0,
        }
    }
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

pub(crate) fn srv_cli_str(is_client: bool) -> String {
    if is_client {
        return "client".to_owned();
    }
    "server".to_owned()
}

pub(crate) struct HandshakeFsm {
    current_flight: Box<dyn Flight>,
    flights: Vec<Packet>,
    retransmit: bool,
    state: State,
    cache: HandshakeCache,
    cfg: HandshakeConfig,

    closed_tx: mpsc::Sender<()>,
    handshake_rx: mpsc::Receiver<()>,
    done_rx: mpsc::Receiver<()>,
}

impl HandshakeFsm {
    pub(crate) fn new(
        state: State,
        cache: HandshakeCache,
        cfg: HandshakeConfig,
        initial_flight: Box<dyn Flight>,
        closed_tx: mpsc::Sender<()>,
        handshake_rx: mpsc::Receiver<()>,
        done_rx: mpsc::Receiver<()>,
    ) -> Self {
        HandshakeFsm {
            current_flight: initial_flight,
            flights: vec![],
            retransmit: false,
            state,
            cache,
            cfg,

            closed_tx,
            handshake_rx,
            done_rx,
        }
    }

    pub(crate) async fn run(
        &mut self,
        c: &mut Conn,
        initial_state: HandshakeState,
    ) -> Result<(), Error> {
        let mut state = initial_state;
        loop {
            trace!(
                "[handshake:{}] {}: {}",
                srv_cli_str(self.state.is_client),
                self.current_flight.to_string(),
                state.to_string()
            );
            if let Some(on_flight_state) = &self.cfg.on_flight_state {
                on_flight_state(&self.current_flight, state);
            }
            state = match state {
                HandshakeState::Preparing => self.prepare(c).await?,
                HandshakeState::Sending => self.send(c).await?,
                HandshakeState::Waiting => self.wait(c).await?,
                HandshakeState::Finished => self.finish(c).await?,
                _ => return Err(ERR_INVALID_FSM_TRANSITION.clone()),
            };
        }
    }

    async fn prepare(&mut self, c: &mut Conn) -> Result<HandshakeState, Error> {
        self.flights = vec![];

        // Prepare flights
        self.retransmit = self.current_flight.has_retransmit();

        let result = self
            .current_flight
            .generate(&mut self.state, &self.cache, &self.cfg)
            .await;

        match result {
            Err((a, mut err)) => {
                if let Some(a) = a {
                    let alert_err = c.notify(a.alert_level, a.alert_description);

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
            Ok(pkts) => self.flights = pkts,
        };

        let epoch = self.cfg.initial_epoch;
        let mut next_epoch = epoch;
        for p in &mut self.flights {
            p.record.record_layer_header.epoch += epoch;
            if p.record.record_layer_header.epoch > next_epoch {
                next_epoch = p.record.record_layer_header.epoch;
            }
            if let Content::Handshake(h) = &mut p.record.content {
                h.handshake_header.message_sequence = self.state.handshake_send_sequence as u16;
                self.state.handshake_send_sequence += 1;
            }
        }
        if epoch != next_epoch {
            trace!(
                "[handshake:{}] -> changeCipherSpec (epoch: {})",
                srv_cli_str(self.state.is_client),
                next_epoch
            );
            c.set_local_epoch(next_epoch);
        }

        Ok(HandshakeState::Sending)
    }
    async fn send(&mut self, c: &mut Conn) -> Result<HandshakeState, Error> {
        // Send flights
        c.write_packets(&mut self.flights).await?;

        if self.current_flight.is_last_send_flight() {
            Ok(HandshakeState::Finished)
        } else {
            Ok(HandshakeState::Waiting)
        }
    }
    async fn wait(&mut self, c: &mut Conn) -> Result<HandshakeState, Error> {
        let mut retransmit_timer = tokio::time::sleep(self.cfg.retransmit_interval);

        loop {
            tokio::select! {
                 _ = self.handshake_rx.recv() =>{
                   let result = self.current_flight.parse( c, &mut self.state, &self.cache, &self.cfg).await;
                   // TODO: drop(handshake_rx)
                   match result {
                        Err((alert, mut err)) => {
                            if let Some(alert) = alert {
                                let alert_err = c.notify(alert.alert_level, alert.alert_description);

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

                _ = &mut retransmit_timer =>{
                    if !self.retransmit {
                        return Ok(HandshakeState::Waiting);
                    }
                    return Ok(HandshakeState::Sending);
                }

                _ = self.done_rx.recv() => {
                    return Err(Error::new("done_rx recv".to_owned()));
                }
            }
        }
    }
    async fn finish(&mut self, c: &mut Conn) -> Result<HandshakeState, Error> {
        let retransmit_timer = tokio::time::sleep(self.cfg.retransmit_interval);

        tokio::select! {
             _ = self.handshake_rx.recv() =>{
               let result = self.current_flight.parse( c, &mut self.state, &self.cache, &self.cfg).await;
                // TODO: drop(handshake_rx)
               match result {
                    Err((alert, mut err)) => {
                        if let Some(alert) = alert {
                            let alert_err = c.notify(alert.alert_level, alert.alert_description);

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

            _ = self.done_rx.recv() => {
                return Err(Error::new("done_rx recv".to_owned()));
            }
        }

        Ok(HandshakeState::Finished)
    }
}
