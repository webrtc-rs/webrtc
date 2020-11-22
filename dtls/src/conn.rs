use crate::cipher_suite::*;
use crate::config::*;
use crate::curve::named_curve::NamedCurve;
use crate::flight::flight0::*;
use crate::flight::flight1::*;
//use crate::flight::flight2::*;
//use crate::flight::flight3::*;
//use crate::flight::flight4::*;
use crate::alert::*;
use crate::application_data::*;
use crate::content::*;
use crate::errors::*;
use crate::extension::extension_use_srtp::*;
use crate::flight::flight5::*;
use crate::flight::flight6::*;
use crate::flight::*;
use crate::fragment_buffer::*;
use crate::handshake::handshake_cache::*;
use crate::handshake::handshake_header::HandshakeHeader;
use crate::handshake::*;
use crate::handshaker::*;
use crate::record_layer::record_layer_header::*;
use crate::record_layer::*;
use crate::signature_hash_algorithm::parse_signature_schemes;
use crate::state::*;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use log::*;

use tokio::net::*;
use tokio::sync::mpsc;
use tokio::time;

use std::io::{BufReader, BufWriter};
use tokio::time::Duration;
use transport::replay_detector::SlidingWindowDetector;
use util::Error;

pub(crate) const INITIAL_TICKER_INTERVAL: time::Duration = time::Duration::from_secs(1);
pub(crate) const COOKIE_LENGTH: usize = 20;
pub(crate) const DEFAULT_NAMED_CURVE: NamedCurve = NamedCurve::X25519;
pub(crate) const INBOUND_BUFFER_SIZE: usize = 8192;
// Default replay protection window is specified by RFC 6347 Section 4.1.2.6
pub(crate) const DEFAULT_REPLAY_PROTECTION_WINDOW: usize = 64;

lazy_static! {
    pub static ref INVALID_KEYING_LABELS: HashMap<&'static str, bool> = {
        let mut map = HashMap::new();
        map.insert("client finished", true);
        map.insert("server finished", true);
        map.insert("master secret", true);
        map.insert("key expansion", true);
        map
    };
}

// Conn represents a DTLS connection
pub(crate) struct Conn {
    //lock           sync.RWMutex     // Internal lock (must not be public)
    next_conn: UdpSocket, // Embedded Conn, typically a udpconn we read/write from
    fragment_buffer: FragmentBuffer, // out-of-order and missing fragment handling
    handshake_cache: HandshakeCache, // caching of handshake messages for verifyData generation
    decrypted_tx: mpsc::Sender<Result<Vec<u8>, Error>>, // Decrypted Application Data or error, pull by calling `Read`
    decrypted_rx: mpsc::Receiver<Result<Vec<u8>, Error>>, // Decrypted Application Data or error, pull by calling `Read`
    state: State,                                         // Internal state

    maximum_transmission_unit: usize,

    handshake_completed_successfully: AtomicBool,

    encrypted_packets: Option<Vec<Vec<u8>>>,

    connection_closed_by_user: bool,
    // closeLock              sync.Mutex
    closed: bool, //  *closer.Closer
    //handshakeLoopsFinished sync.WaitGroup

    //readDeadline  :deadline.Deadline,
    //writeDeadline :deadline.Deadline,

    //log logging.LeveledLogger
    /*
    reading               chan struct{}
    handshakeRecv         chan chan struct{}
    cancelHandshaker      func()
    cancelHandshakeReader func()
    */
    //fsm: HandshakeFsm,
    replay_protection_window: usize,
}

unsafe impl std::marker::Send for Conn {}
unsafe impl std::marker::Sync for Conn {}

impl Conn {
    pub async fn new(
        next_conn: UdpSocket,
        config: &mut Config,
        is_client: bool,
        initial_state: Option<State>,
    ) -> Result<Self, Error> {
        validate_config(config)?;

        let local_cipher_suites: Vec<CipherSuiteID> = parse_cipher_suites(
            &config.cipher_suites,
            config.psk.is_none(),
            config.psk.is_some(),
        )?
        .iter()
        .map(|cs| cs.id())
        .collect();

        let sigs: Vec<u16> = config.signature_schemes.iter().map(|x| *x as u16).collect();
        let local_signature_schemes = parse_signature_schemes(&sigs, config.insecure_hashes)?;

        let retransmit_interval = if config.flight_interval != Duration::from_secs(0) {
            config.flight_interval
        } else {
            INITIAL_TICKER_INTERVAL
        };

        /*
           loggerFactory := config.LoggerFactory
           if loggerFactory == nil {
               loggerFactory = logging.NewDefaultLoggerFactory()
           }

           logger := loggerFactory.NewLogger("dtls")
        */
        let maximum_transmission_unit = if config.mtu == 0 {
            DEFAULT_MTU
        } else {
            config.mtu
        };

        let replay_protection_window = if config.replay_protection_window == 0 {
            DEFAULT_REPLAY_PROTECTION_WINDOW
        } else {
            config.replay_protection_window
        };

        let (decrypted_tx, decrypted_rx) = mpsc::channel(1);

        let mut c = Conn {
            next_conn,
            fragment_buffer: FragmentBuffer::new(),
            handshake_cache: HandshakeCache::new(),
            decrypted_tx,
            decrypted_rx,
            state: State {
                is_client,
                ..Default::default()
            },
            maximum_transmission_unit,
            handshake_completed_successfully: AtomicBool::new(false),
            encrypted_packets: None,
            connection_closed_by_user: false,
            replay_protection_window,
            closed: false,
        };

        //c.set_remote_epoch(0);
        //c.set_local_epoch(0);

        let server_name = config.server_name.clone();
        // Use host from conn address when server_name is not provided
        // TODO:
        /*if is_client && server_name == "" && next_conn.RemoteAddr() != nil {
            remoteAddr := nextConn.RemoteAddr().String()
            var host string
            host, _, err = net.SplitHostPort(remoteAddr)
            if err != nil {
                server_name = remoteAddr
            } else {
                server_name = host
            }
        }*/

        let _hs_cfg = HandshakeConfig {
            local_psk_callback: config.psk.take(),
            local_psk_identity_hint: config.psk_identity_hint.clone(),
            local_cipher_suites,
            local_signature_schemes,
            extended_master_secret: config.extended_master_secret,
            local_srtp_protection_profiles: config.srtp_protection_profiles.clone(),
            server_name,
            client_auth: config.client_auth,
            local_certificates: config.certificates.clone(),
            insecure_skip_verify: config.insecure_skip_verify,
            verify_peer_certificate: config.verify_peer_certificate.take(),
            //rootCAs: config.RootCAs,
            //clientCAs: config.ClientCAs,
            retransmit_interval,
            //log: logger,
            initial_epoch: 0,
            ..Default::default()
        };

        let (_initial_flight, _initial_fsm_state) = if let Some(state) = initial_state {
            c.state = state;
            if is_client {
                (
                    Box::new(Flight5 {}) as Box<dyn Flight>,
                    HandshakeState::Finished,
                )
            } else {
                (
                    Box::new(Flight6 {}) as Box<dyn Flight>,
                    HandshakeState::Finished,
                )
            }
        } else if is_client {
            (
                Box::new(Flight1 {}) as Box<dyn Flight>,
                HandshakeState::Preparing,
            )
        } else {
            (
                Box::new(Flight0 {}) as Box<dyn Flight>,
                HandshakeState::Preparing,
            )
        };

        // Do handshake
        //Todo: c.handshake(ctx, hsCfg, initialFlight, initialFSMState)?

        //c.log.Trace("Handshake Completed")

        Ok(c)
    }

    async fn handshake(
        &mut self,
        cfg: HandshakeConfig,
        initial_flight: Box<dyn Flight>,
        _initial_state: HandshakeState,
    ) -> Result<(), Error> {
        let (closed_tx, _closed_rx) = mpsc::channel(1);
        let (_handshake_tx, handshake_rx) = mpsc::channel(1);
        let (_done_tx, done_rx) = mpsc::channel(1);
        //let (first_err_tx, mut first_err_rx) = mpsc::channel(1);

        let _fsm = HandshakeFsm::new(
            self.state.clone(),
            self.handshake_cache.clone(),
            cfg,
            initial_flight,
            closed_tx,
            handshake_rx,
            done_rx,
        );

        //TODO:
        /*cfg.onFlightState = func(f flightVal, s handshakeState) {
            if s == handshakeFinished && !c.is_handshake_completed_successfully() {
                c.set_handshake_completed_successfully()
                close(done)
            }
        }*/

        // Handshake routine should be live until close.
        // The other party may request retransmission of the last flight to cope with packet drop.
        /*tokio::spawn(async move {
            let result = fsm.run(/*ctxHs,*/ c, initial_state).await;
            if let Err(err) = result {
                if err != *ERR_CONTEXT_CANCELED {
                    let _ = first_err_tx.send(err).await;
                }
            }
            //TODO: c.handshakeLoopsFinished.Done()
        });*/

        tokio::spawn(async move {});

        //tokio::select! {
        //_ = first_err_rx.recv() => {}
        //}

        Ok(())
    }

    // Read reads data from the connection.
    pub async fn read(
        &mut self,
        _p: &mut [u8],
        duration: Option<Duration>,
    ) -> Result<usize, Error> {
        if !self.is_handshake_completed_successfully() {
            return Err(ERR_HANDSHAKE_IN_PROGRESS.clone());
        }

        //TODO
        if let Some(_d) = duration {
        } else {
        }
        /*select {
        case <-c.readDeadline.Done():
            return 0, errDeadlineExceeded
        case out, ok := <-c.decrypted:
            if !ok {
                return 0, io.EOF
            }
            switch val := out.(type) {
            case ([]byte):
                if len(p) < len(val) {
                    return 0, errBufferTooSmall
                }
                copy(p, val)
                return len(val), nil
            case (error):
                return 0, val
            }
        }*/
        Ok(0)
    }

    // Write writes len(p) bytes from p to the DTLS connection
    pub async fn write(&mut self, p: &[u8], _duration: Option<Duration>) -> Result<usize, Error> {
        if self.is_connection_closed() {
            return Err(ERR_CONN_CLOSED.clone());
        }

        if !self.is_handshake_completed_successfully() {
            return Err(ERR_HANDSHAKE_IN_PROGRESS.clone());
        }

        self.write_packets(&mut [Packet {
            record: RecordLayer {
                record_layer_header: RecordLayerHeader {
                    epoch: self.get_local_epoch(),
                    protocol_version: PROTOCOL_VERSION1_2,
                    ..Default::default()
                },
                content: Content::ApplicationData(ApplicationData { data: p.to_vec() }),
            },
            should_encrypt: true,
            reset_local_sequence_number: false,
        }])
        .await?;

        Ok(p.len())
    }

    // Close closes the connection.
    pub fn close(&self) -> Result<(), Error> {
        //err := c.close(true)
        //c.handshakeLoopsFinished.Wait()
        //return err
        Ok(())
    }

    // ConnectionState returns basic DTLS details about the connection.
    // Note that this replaced the `Export` function of v1.
    pub fn connection_state(&self) -> State {
        //c.lock.RLock()
        //defer c.lock.RUnlock()
        self.state.clone()
    }

    // selected_srtpprotection_profile returns the selected SRTPProtectionProfile
    pub fn selected_srtpprotection_profile(&self) -> SRTPProtectionProfile {
        //c.lock.RLock()
        //defer c.lock.RUnlock()

        self.state.srtp_protection_profile
    }

    pub(crate) async fn notify(
        &mut self,
        level: AlertLevel,
        desc: AlertDescription,
    ) -> Result<(), Error> {
        self.write_packets(&mut [Packet {
            record: RecordLayer {
                record_layer_header: RecordLayerHeader {
                    epoch: self.get_local_epoch(),
                    protocol_version: PROTOCOL_VERSION1_2,
                    ..Default::default()
                },
                content: Content::Alert(Alert {
                    alert_level: level,
                    alert_description: desc,
                }),
            },
            should_encrypt: self.is_handshake_completed_successfully(),
            reset_local_sequence_number: false,
        }])
        .await
    }

    pub(crate) async fn write_packets(&mut self, pkts: &mut [Packet]) -> Result<(), Error> {
        //c.lock.Lock()
        //defer c.lock.Unlock()

        let mut raw_packets = vec![];
        for p in pkts {
            if let Content::Handshake(h) = &p.record.content {
                let mut handshake_raw = vec![];
                {
                    let mut writer = BufWriter::<&mut Vec<u8>>::new(handshake_raw.as_mut());
                    p.record.marshal(&mut writer)?;
                }
                trace!(
                    "[handshake:{}] -> {} (epoch: {}, seq: {})",
                    srv_cli_str(self.state.is_client),
                    h.handshake_header.handshake_type.to_string(),
                    p.record.record_layer_header.epoch,
                    h.handshake_header.message_sequence
                );
                self.handshake_cache
                    .push(
                        handshake_raw[RECORD_LAYER_HEADER_SIZE..].to_vec(),
                        p.record.record_layer_header.epoch,
                        h.handshake_header.message_sequence,
                        h.handshake_header.handshake_type,
                        self.state.is_client,
                    )
                    .await;

                let raw_handshake_packets = self.process_handshake_packet(p, h)?;
                raw_packets.extend_from_slice(&raw_handshake_packets);
            } else {
                let raw_packet = self.process_packet(p)?;
                raw_packets.push(raw_packet);
            }
        }
        if raw_packets.is_empty() {
            return Ok(());
        }

        let compacted_raw_packets =
            compact_raw_packets(&raw_packets, self.maximum_transmission_unit);

        for compacted_raw_packets in &compacted_raw_packets {
            self.next_conn.send(compacted_raw_packets).await?;
        }

        Ok(())
    }

    fn process_packet(&mut self, p: &mut Packet) -> Result<Vec<u8>, Error> {
        let epoch = p.record.record_layer_header.epoch as usize;
        while self.state.local_sequence_number.len() <= epoch {
            self.state.local_sequence_number.push(0);
        }
        //TODO: seq := atomic.AddUint64(&c.state.localSequenceNumber[epoch], 1) - 1
        self.state.local_sequence_number[epoch] += 1;
        let seq = self.state.local_sequence_number[epoch] - 1;
        if seq > MAX_SEQUENCE_NUMBER {
            // RFC 6347 Section 4.1.0
            // The implementation must either abandon an association or rehandshake
            // prior to allowing the sequence number to wrap.
            return Err(ERR_SEQUENCE_NUMBER_OVERFLOW.clone());
        }
        p.record.record_layer_header.sequence_number = seq;

        let mut raw_packet = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(raw_packet.as_mut());
            p.record.marshal(&mut writer)?;
        }

        if p.should_encrypt {
            if let Some(cipher_suite) = &self.state.cipher_suite {
                raw_packet = cipher_suite.encrypt(&p.record.record_layer_header, &raw_packet)?;
            }
        }

        Ok(raw_packet)
    }

    fn process_handshake_packet(
        &mut self,
        p: &Packet,
        h: &Handshake,
    ) -> Result<Vec<Vec<u8>>, Error> {
        let mut raw_packets = vec![];

        let handshake_fragments = self.fragment_handshake(h)?;

        let epoch = p.record.record_layer_header.epoch as usize;
        while self.state.local_sequence_number.len() <= epoch {
            self.state.local_sequence_number.push(0);
        }

        for handshake_fragment in &handshake_fragments {
            //seq := atomic.AddUint64(&c.state.localSequenceNumber[epoch], 1) - 1
            self.state.local_sequence_number[epoch] += 1;
            let seq = self.state.local_sequence_number[epoch] - 1;
            if seq > MAX_SEQUENCE_NUMBER {
                return Err(ERR_SEQUENCE_NUMBER_OVERFLOW.clone());
            }

            let record_layer_header = RecordLayerHeader {
                protocol_version: p.record.record_layer_header.protocol_version,
                content_type: p.record.record_layer_header.content_type,
                content_len: handshake_fragment.len() as u16,
                epoch: p.record.record_layer_header.epoch,
                sequence_number: seq,
            };

            let mut record_layer_header_bytes = vec![];
            {
                let mut writer = BufWriter::<&mut Vec<u8>>::new(record_layer_header_bytes.as_mut());
                record_layer_header.marshal(&mut writer)?;
            }

            //p.record.record_layer_header = record_layer_header;

            let mut raw_packet = vec![];
            raw_packet.extend_from_slice(&record_layer_header_bytes);
            raw_packet.extend_from_slice(&handshake_fragment);
            if p.should_encrypt {
                if let Some(cipher_suite) = &self.state.cipher_suite {
                    raw_packet = cipher_suite.encrypt(&record_layer_header, &raw_packet)?;
                }
            }

            raw_packets.push(raw_packet);
        }

        Ok(raw_packets)
    }

    fn fragment_handshake(&self, h: &Handshake) -> Result<Vec<Vec<u8>>, Error> {
        let mut content = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(content.as_mut());
            h.handshake_message.marshal(&mut writer)?;
        }

        let mut fragmented_handshakes = vec![];

        let mut content_fragments = split_bytes(&content, self.maximum_transmission_unit);
        if content_fragments.is_empty() {
            content_fragments = vec![vec![]];
        }

        let mut offset = 0;
        for content_fragment in &content_fragments {
            let content_fragment_len = content_fragment.len();

            let handshake_header_fragment = HandshakeHeader {
                handshake_type: h.handshake_header.handshake_type,
                length: h.handshake_header.length,
                message_sequence: h.handshake_header.message_sequence,
                fragment_offset: offset as u32,
                fragment_length: content_fragment_len as u32,
            };

            offset += content_fragment_len;

            let mut handshake_header_fragment_raw = vec![];
            {
                let mut writer =
                    BufWriter::<&mut Vec<u8>>::new(handshake_header_fragment_raw.as_mut());
                handshake_header_fragment.marshal(&mut writer)?;
            }

            let mut fragmented_handshake = vec![];
            fragmented_handshake.extend_from_slice(&handshake_header_fragment_raw);
            fragmented_handshake.extend_from_slice(&content_fragment);

            fragmented_handshakes.push(fragmented_handshake);
        }

        Ok(fragmented_handshakes)
    }

    fn set_handshake_completed_successfully(&mut self) {
        self.handshake_completed_successfully
            .store(true, Ordering::Relaxed);
    }

    fn is_handshake_completed_successfully(&self) -> bool {
        self.handshake_completed_successfully
            .load(Ordering::Relaxed)
    }

    //pub(crate) fn recv_handshake(&self) -> mpsc::Receiver<()> {}

    pub(crate) async fn handle_queued_packets(&mut self) -> Result<(), Error> {
        if let Some(pkts) = self.encrypted_packets.take() {
            for p in pkts {
                let (_, alert, mut err) = self.handle_incoming_packet(p, false).await; // don't re-enqueue
                if let Some(alert) = alert {
                    let alert_err = self
                        .notify(alert.alert_level, alert.alert_description)
                        .await;
                    if let Err(alert_err) = alert_err {
                        if err.is_none() {
                            err = Some(alert_err);
                        }
                    }

                    if alert.alert_level == AlertLevel::Fatal
                        || alert.alert_description == AlertDescription::CloseNotify
                    {
                        return Err(Error::new("Alert is Fatal or Close Notify".to_owned()));
                    }
                }

                if let Some(err) = err {
                    return Err(err);
                }
            }
        }

        Ok(())
    }

    async fn handle_incoming_packet(
        &mut self,
        mut buf: Vec<u8>,
        enqueue: bool,
    ) -> (bool, Option<Alert>, Option<Error>) {
        let mut reader = BufReader::new(buf.as_slice());
        let h = match RecordLayerHeader::unmarshal(&mut reader) {
            Ok(h) => h,
            Err(err) => {
                // Decode error must be silently discarded
                // [RFC6347 Section-4.1.2.7]
                debug!("discarded broken packet: {}", err);
                return (false, None, None);
            }
        };

        // Validate epoch
        let remote_epoch = self.get_remote_epoch();
        if h.epoch > remote_epoch {
            if h.epoch > remote_epoch + 1 {
                debug!(
                    "discarded future packet (epoch: {}, seq: {})",
                    h.epoch, h.sequence_number,
                );
                return (false, None, None);
            }
            if enqueue {
                debug!("received packet of next epoch, queuing packet");
                if let Some(encrypted_packets) = &mut self.encrypted_packets {
                    encrypted_packets.push(buf);
                }
            }
            return (false, None, None);
        }

        // Anti-replay protection
        while self.state.replay_detector.len() <= h.epoch as usize {
            self.state
                .replay_detector
                .push(Box::new(SlidingWindowDetector::new(
                    self.replay_protection_window,
                    MAX_SEQUENCE_NUMBER,
                )));
        }

        let ok = self.state.replay_detector[h.epoch as usize].check(h.sequence_number);
        if !ok {
            debug!(
                "discarded duplicated packet (epoch: {}, seq: {})",
                h.epoch, h.sequence_number,
            );
            return (false, None, None);
        }

        // Decrypt
        if h.epoch != 0 {
            let invalid_cipher_suite = if self.state.cipher_suite.is_none() {
                true
            } else if let Some(cipher_suite) = &self.state.cipher_suite {
                !cipher_suite.is_initialized()
            } else {
                false
            };
            if invalid_cipher_suite {
                if enqueue {
                    if let Some(encrypted_packets) = &mut self.encrypted_packets {
                        encrypted_packets.push(buf);
                    }
                    debug!("handshake not finished, queuing packet");
                }
                return (false, None, None);
            }

            if let Some(cipher_suite) = &self.state.cipher_suite {
                buf = match cipher_suite.decrypt(&buf) {
                    Ok(buf) => buf,
                    Err(err) => {
                        debug!(
                            "{}: decrypt failed: {}",
                            srv_cli_str(self.state.is_client),
                            err
                        );
                        return (false, None, None);
                    }
                };
            }
        }

        let is_handshake = match self.fragment_buffer.push(&buf) {
            Ok(is_handshake) => is_handshake,
            Err(err) => {
                // Decode error must be silently discarded
                // [RFC6347 Section-4.1.2.7]
                debug!("defragment failed: {}", err);
                return (false, None, None);
            }
        };
        if is_handshake {
            self.state.replay_detector[h.epoch as usize].accept();
            while let Ok((out, epoch)) = self.fragment_buffer.pop() {
                let mut reader = BufReader::new(out.as_slice());
                let raw_handshake = match Handshake::unmarshal(&mut reader) {
                    Ok(h) => h,
                    Err(err) => {
                        debug!(
                            "{}: handshake parse failed: {}",
                            srv_cli_str(self.state.is_client),
                            err
                        );
                        continue;
                    }
                };

                self.handshake_cache
                    .push(
                        out,
                        epoch,
                        raw_handshake.handshake_header.message_sequence,
                        raw_handshake.handshake_header.handshake_type,
                        !self.state.is_client,
                    )
                    .await;
            }

            return (true, None, None);
        }

        let mut reader = BufReader::new(buf.as_slice());
        let r = match RecordLayer::unmarshal(&mut reader) {
            Ok(r) => r,
            Err(err) => {
                return (
                    false,
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::DecodeError,
                    }),
                    Some(err),
                );
            }
        };

        match r.content {
            Content::Alert(mut a) => {
                trace!(
                    "{}: <- {}",
                    srv_cli_str(self.state.is_client),
                    a.to_string()
                );
                if a.alert_description == AlertDescription::CloseNotify {
                    // Respond with a close_notify [RFC5246 Section 7.2.1]
                    a = Alert {
                        alert_level: AlertLevel::Warning,
                        alert_description: AlertDescription::CloseNotify,
                    };
                }
                self.state.replay_detector[h.epoch as usize].accept();
                return (
                    false,
                    Some(a),
                    Some(Error::new(format!("Error of Alert {}", a.to_string()))),
                ); //TODO: &errAlert { content });
            }
            Content::ChangeCipherSpec(_) => {
                let invalid_cipher_suite = if self.state.cipher_suite.is_none() {
                    true
                } else if let Some(cipher_suite) = &self.state.cipher_suite {
                    !cipher_suite.is_initialized()
                } else {
                    false
                };

                if invalid_cipher_suite {
                    if enqueue {
                        if let Some(encrypted_packets) = &mut self.encrypted_packets {
                            encrypted_packets.push(buf);
                        }
                        debug!("CipherSuite not initialized, queuing packet");
                    }
                    return (false, None, None);
                }

                let new_remote_epoch = h.epoch + 1;
                trace!(
                    "{}: <- ChangeCipherSpec (epoch: {})",
                    srv_cli_str(self.state.is_client),
                    new_remote_epoch
                );

                if self.get_remote_epoch() + 1 == new_remote_epoch {
                    self.set_remote_epoch(new_remote_epoch);
                    self.state.replay_detector[h.epoch as usize].accept();
                }
            }
            Content::ApplicationData(a) => {
                if h.epoch == 0 {
                    return (
                        false,
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::UnexpectedMessage,
                        }),
                        Some(ERR_APPLICATION_DATA_EPOCH_ZERO.clone()),
                    );
                }

                self.state.replay_detector[h.epoch as usize].accept();

                let _ = self.decrypted_tx.send(Ok(a.data)).await;
                //TODO
                /*select {
                    case self.decrypted < - content.data:
                    case < -c.closed.Done():
                }*/
            }
            _ => {
                return (
                    false,
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::UnexpectedMessage,
                    }),
                    Some(ERR_UNHANDLED_CONTEXT_TYPE.clone()),
                );
            }
        };

        (false, None, None)
    }

    fn is_connection_closed(&self) -> bool {
        /*select {
        case <-c.closed.Done():
            return true
        default:
            return false
        }*/
        self.closed
    }

    pub(crate) fn set_local_epoch(&mut self, epoch: u16) {
        self.state.local_epoch.store(epoch, Ordering::Relaxed);
    }

    pub(crate) fn get_local_epoch(&self) -> u16 {
        self.state.local_epoch.load(Ordering::Relaxed)
    }

    pub(crate) fn set_remote_epoch(&mut self, epoch: u16) {
        self.state.remote_epoch.store(epoch, Ordering::Relaxed);
    }

    pub(crate) fn get_remote_epoch(&self) -> u16 {
        self.state.remote_epoch.load(Ordering::Relaxed)
    }
}

fn compact_raw_packets(raw_packets: &[Vec<u8>], maximum_transmission_unit: usize) -> Vec<Vec<u8>> {
    let mut combined_raw_packets = vec![];
    let mut current_combined_raw_packet = vec![];

    for raw_packet in raw_packets {
        if !current_combined_raw_packet.is_empty()
            && current_combined_raw_packet.len() + raw_packet.len() >= maximum_transmission_unit
        {
            combined_raw_packets.push(current_combined_raw_packet);
            current_combined_raw_packet = vec![];
        }
        current_combined_raw_packet.extend_from_slice(raw_packet);
    }

    combined_raw_packets.push(current_combined_raw_packet);

    combined_raw_packets
}

fn split_bytes(bytes: &[u8], split_len: usize) -> Vec<Vec<u8>> {
    let mut splits = vec![];
    let num_bytes = bytes.len();
    for i in (0..num_bytes).step_by(split_len) {
        let mut j = i + split_len;
        if j > num_bytes {
            j = num_bytes;
        }

        splits.push(bytes[i..j].to_vec());
    }

    splits
}
