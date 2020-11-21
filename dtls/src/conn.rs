use crate::cipher_suite::*;
use crate::config::*;
use crate::curve::named_curve::NamedCurve;
use crate::flight::flight0::*;
use crate::flight::flight1::*;
//use crate::flight::flight2::*;
//use crate::flight::flight3::*;
//use crate::flight::flight4::*;
use crate::alert::*;
use crate::flight::flight5::*;
use crate::flight::flight6::*;
use crate::flight::*;
use crate::fragment_buffer::*;
use crate::handshake::handshake_cache::*;
use crate::handshaker::*;
use crate::state::*;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::net::*;
//use tokio::sync::mpsc;
use tokio::time;

use crate::signature_hash_algorithm::parse_signature_schemes;
use tokio::time::Duration;
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
    //decrypted      chan interface{} // Decrypted Application Data or error, pull by calling `Read`
    state: State, // Internal state

    maximum_transmission_unit: usize,

    handshake_completed_successfully: AtomicBool,

    encrypted_packets: Vec<Vec<u8>>,

    connection_closed_by_user: bool,
    // closeLock              sync.Mutex
    //closed                 *closer.Closer
    //handshakeLoopsFinished sync.WaitGroup

    //readDeadline  :deadline.Deadline,
    //writeDeadline :deadline.Deadline,

    //log logging.LeveledLogger
    /*
    reading               chan struct{}
    handshakeRecv         chan chan struct{}
    cancelHandshaker      func()
    cancelHandshakeReader func()

    fsm *handshakeFSM*/
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

        let mut c = Conn {
            next_conn,
            fragment_buffer: FragmentBuffer::new(),
            handshake_cache: HandshakeCache::new(),
            state: State {
                is_client,
                ..Default::default()
            },
            maximum_transmission_unit,
            handshake_completed_successfully: AtomicBool::new(false),
            encrypted_packets: vec![],
            connection_closed_by_user: false,
            replay_protection_window,
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

    pub(crate) fn notify(&self, _level: AlertLevel, _desc: AlertDescription) -> Result<(), Error> {
        Ok(())
    }

    pub(crate) fn write_packets(&self, _packets: &[Packet]) -> Result<(), Error> {
        Ok(())
    }

    //pub(crate) fn recv_handshake(&self) -> mpsc::Receiver<()> {}

    pub(crate) fn handle_queued_packets(&self) -> Result<(), Error> {
        Ok(())
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
