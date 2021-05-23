mod association_internal;
mod association_stats;

use crate::chunk::chunk_abort::ChunkAbort;
use crate::chunk::chunk_cookie_ack::ChunkCookieAck;
use crate::chunk::chunk_cookie_echo::ChunkCookieEcho;
use crate::chunk::chunk_error::ChunkError;
use crate::chunk::chunk_forward_tsn::{ChunkForwardTsn, ChunkForwardTsnStream};
use crate::chunk::chunk_heartbeat::ChunkHeartbeat;
use crate::chunk::chunk_heartbeat_ack::ChunkHeartbeatAck;
use crate::chunk::chunk_init::ChunkInit;
use crate::chunk::chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier};
use crate::chunk::chunk_reconfig::ChunkReconfig;
use crate::chunk::chunk_selective_ack::ChunkSelectiveAck;
use crate::chunk::chunk_shutdown::ChunkShutdown;
use crate::chunk::chunk_shutdown_ack::ChunkShutdownAck;
use crate::chunk::chunk_shutdown_complete::ChunkShutdownComplete;
use crate::chunk::chunk_type::*;
use crate::chunk::Chunk;
use crate::error::Error;
use crate::error_cause::*;
use crate::packet::Packet;
use crate::param::param_heartbeat_info::ParamHeartbeatInfo;
use crate::param::param_outgoing_reset_request::ParamOutgoingResetRequest;
use crate::param::param_reconfig_response::{ParamReconfigResponse, ReconfigResult};
use crate::param::param_state_cookie::ParamStateCookie;
use crate::param::param_supported_extensions::ParamSupportedExtensions;
use crate::param::Param;
use crate::queue::control_queue::ControlQueue;
use crate::queue::payload_queue::PayloadQueue;
use crate::queue::pending_queue::PendingQueue;
use crate::stream::*;
use crate::timer::ack_timer::*;
use crate::timer::rtx_timer::*;
use crate::util::*;

use association_internal::*;
use association_stats::*;

use bytes::Bytes;
use rand::random;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{broadcast, mpsc, Mutex, Notify};
use util::Conn;
//use async_trait::async_trait;

pub(crate) const RECEIVE_MTU: usize = 8192;
/// MTU for inbound packet (from DTLS)
pub(crate) const INITIAL_MTU: u32 = 1228;
/// initial MTU for outgoing packets (to DTLS)
pub(crate) const INITIAL_RECV_BUF_SIZE: u32 = 1024 * 1024;
pub(crate) const COMMON_HEADER_SIZE: u32 = 12;
pub(crate) const DATA_CHUNK_HEADER_SIZE: u32 = 16;
pub(crate) const DEFAULT_MAX_MESSAGE_SIZE: u32 = 65536;

/// other constants
pub(crate) const ACCEPT_CH_SIZE: usize = 16;

/// association state enums
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum AssociationState {
    Closed = 0,
    CookieWait = 1,
    CookieEchoed = 2,
    Established = 3,
    ShutdownAckSent = 4,
    ShutdownPending = 5,
    ShutdownReceived = 6,
    ShutdownSent = 7,
}

impl From<u8> for AssociationState {
    fn from(v: u8) -> AssociationState {
        match v {
            1 => AssociationState::CookieWait,
            2 => AssociationState::CookieEchoed,
            3 => AssociationState::Established,
            4 => AssociationState::ShutdownAckSent,
            5 => AssociationState::ShutdownPending,
            6 => AssociationState::ShutdownReceived,
            7 => AssociationState::ShutdownSent,
            _ => AssociationState::Closed,
        }
    }
}

impl fmt::Display for AssociationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            AssociationState::Closed => "Closed",
            AssociationState::CookieWait => "CookieWait",
            AssociationState::CookieEchoed => "CookieEchoed",
            AssociationState::Established => "Established",
            AssociationState::ShutdownPending => "ShutdownPending",
            AssociationState::ShutdownSent => "ShutdownSent",
            AssociationState::ShutdownReceived => "ShutdownReceived",
            AssociationState::ShutdownAckSent => "ShutdownAckSent",
        };
        write!(f, "{}", s)
    }
}

/// retransmission timer IDs
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum RtxTimerId {
    T1Init,
    T1Cookie,
    T2Shutdown,
    T3RTX,
    Reconfig,
}

impl Default for RtxTimerId {
    fn default() -> Self {
        RtxTimerId::T1Init
    }
}

impl fmt::Display for RtxTimerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RtxTimerId::T1Init => "T1Init",
            RtxTimerId::T1Cookie => "T1Cookie",
            RtxTimerId::T2Shutdown => "T2Shutdown",
            RtxTimerId::T3RTX => "T3RTX",
            RtxTimerId::Reconfig => "Reconfig",
        };
        write!(f, "{}", s)
    }
}

/// ack mode (for testing)
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum AckMode {
    Normal,
    NoDelay,
    AlwaysDelay,
}
impl Default for AckMode {
    fn default() -> Self {
        AckMode::Normal
    }
}

impl fmt::Display for AckMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            AckMode::Normal => "Normal",
            AckMode::NoDelay => "NoDelay",
            AckMode::AlwaysDelay => "AlwaysDelay",
        };
        write!(f, "{}", s)
    }
}

/// ack transmission state
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum AckState {
    Idle,      // ack timer is off
    Immediate, // will send ack immediately
    Delay,     // ack timer is on (ack is being delayed)
}

impl Default for AckState {
    fn default() -> Self {
        AckState::Idle
    }
}

impl fmt::Display for AckState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            AckState::Idle => "Idle",
            AckState::Immediate => "Immediate",
            AckState::Delay => "Delay",
        };
        write!(f, "{}", s)
    }
}

/// Config collects the arguments to create_association construction into
/// a single structure
pub struct Config {
    pub net_conn: Arc<dyn Conn + Send + Sync>,
    pub max_receive_buffer_size: u32,
    pub max_message_size: u32,
}

///Association represents an SCTP association
///13.2.  Parameters Necessary per Association (i.e., the TCB)
///Peer : Tag value to be sent in every packet and is received
///Verification: in the INIT or INIT ACK chunk.
///Tag :
///
///My : Tag expected in every inbound packet and sent in the
///Verification: INIT or INIT ACK chunk.
///
///Tag :
///State : A state variable indicating what state the association
/// : is in, i.e., COOKIE-WAIT, COOKIE-ECHOED, ESTABLISHED,
/// : SHUTDOWN-PENDING, SHUTDOWN-SENT, SHUTDOWN-RECEIVED,
/// : SHUTDOWN-ACK-SENT.
///
/// No Closed state is illustrated since if a
/// association is Closed its TCB SHOULD be removed.
pub struct Association {
    name: String,
    state: Arc<AtomicU8>,
    max_message_size: Arc<AtomicU32>,
    inflight_queue_length: Arc<AtomicUsize>,
    will_send_shutdown: Arc<AtomicBool>,
    awake_write_loop_ch: Arc<Notify>,
    close_loop_ch_rx: broadcast::Receiver<()>,
    accept_ch_rx: Option<mpsc::Receiver<Arc<Stream>>>,

    net_conn: Arc<dyn Conn + Send + Sync>,
    bytes_received: Arc<AtomicUsize>,
    bytes_sent: Arc<AtomicUsize>,

    association_internal: Arc<Mutex<AssociationInternal>>,
}

impl Association {
    /// server accepts a SCTP stream over a conn
    pub async fn server(config: Config) -> Result<Self, Error> {
        let net_conn = Arc::clone(&config.net_conn);
        let mut ai = AssociationInternal::new(config, false).await?;
        let close_loop_ch_rx = if let Some(close_loop_ch_tx) = &ai.close_loop_ch_tx {
            close_loop_ch_tx.subscribe()
        } else {
            return Err(Error::ErrAssociationInitFailed);
        };

        Ok(Association {
            name: ai.name.clone(),
            state: ai.state.clone(),
            max_message_size: ai.max_message_size.clone(),
            inflight_queue_length: ai.inflight_queue_length.clone(),
            will_send_shutdown: ai.will_send_shutdown.clone(),
            awake_write_loop_ch: ai.awake_write_loop_ch.clone(),
            close_loop_ch_rx,
            accept_ch_rx: ai.accept_ch_rx.take(),
            net_conn,
            bytes_received: Arc::new(AtomicUsize::new(0)),
            bytes_sent: Arc::new(AtomicUsize::new(0)),

            association_internal: Arc::new(Mutex::new(ai)),
        })
    }

    /// Client opens a SCTP stream over a conn
    pub async fn client(config: Config) -> Result<Self, Error> {
        let net_conn = Arc::clone(&config.net_conn);
        let mut ai = AssociationInternal::new(config, true).await?;
        let close_loop_ch_rx = if let Some(close_loop_ch_tx) = &ai.close_loop_ch_tx {
            close_loop_ch_tx.subscribe()
        } else {
            return Err(Error::ErrAssociationInitFailed);
        };

        Ok(Association {
            name: ai.name.clone(),
            state: ai.state.clone(),
            max_message_size: ai.max_message_size.clone(),
            inflight_queue_length: ai.inflight_queue_length.clone(),
            will_send_shutdown: ai.will_send_shutdown.clone(),
            awake_write_loop_ch: ai.awake_write_loop_ch.clone(),
            close_loop_ch_rx,
            accept_ch_rx: ai.accept_ch_rx.take(),
            net_conn,
            bytes_received: Arc::new(AtomicUsize::new(0)),
            bytes_sent: Arc::new(AtomicUsize::new(0)),

            association_internal: Arc::new(Mutex::new(ai)),
        })
    }

    /// Shutdown initiates the shutdown sequence. The method blocks until the
    /// shutdown sequence is completed and the connection is closed, or until the
    /// passed context is done, in which case the context's error is returned.
    pub async fn shutdown(&mut self) -> Result<(), Error> {
        log::debug!("[{}] closing association..", self.name);

        let state = self.get_state();
        if state != AssociationState::Established {
            return Err(Error::ErrShutdownNonEstablished);
        }

        // Attempt a graceful shutdown.
        self.set_state(AssociationState::ShutdownPending);

        if self.inflight_queue_length.load(Ordering::SeqCst) == 0 {
            // No more outstanding, send shutdown.
            self.will_send_shutdown.store(true, Ordering::SeqCst);
            self.awake_write_loop_ch.notify_one();
            self.set_state(AssociationState::ShutdownSent);
        }

        let _ = self.close_loop_ch_rx.recv().await;

        Ok(())
    }

    /// Close ends the SCTP Association and cleans up any state
    pub async fn close(&self) -> Result<(), Error> {
        log::debug!("[{}] closing association..", self.name);

        let mut ai = self.association_internal.lock().await;
        ai.close().await
    }

    async fn read_loop(
        name: String,
        bytes_received: Arc<AtomicUsize>,
        net_conn: Arc<dyn Conn + Send + Sync>,
        mut close_loop_ch: broadcast::Receiver<()>,
        association_internal: Arc<Mutex<AssociationInternal>>,
    ) {
        log::debug!("[{}] read_loop entered", name);

        let mut buffer = vec![0u8; RECEIVE_MTU];
        let mut done = false;
        let mut n;
        while !done {
            tokio::select! {
                _ = close_loop_ch.recv() => break,
                result = net_conn.recv(&mut buffer) => {
                    match result {
                        Ok(m) => {
                            n=m;
                        }
                        Err(err) => {
                            log::warn!("[{}] failed to read packets on net_conn: {}", name, err);
                            break;
                        }
                    }
                }
            };

            // Make a buffer sized to what we read, then copy the data we
            // read from the underlying transport. We do this because the
            // user data is passed to the reassembly queue without
            // copying.
            let inbound = Bytes::from(buffer[..n].to_vec());
            bytes_received.fetch_add(n, Ordering::SeqCst);

            {
                let mut ai = association_internal.lock().await;
                if let Err(err) = ai.handle_inbound(&inbound).await {
                    log::warn!("[{}] failed to handle_inbound: {:?}", name, err);
                    done = true;
                }
            }
        }

        {
            let mut ai = association_internal.lock().await;
            if let Err(err) = ai.close().await {
                log::warn!("[{}] failed to close association: {:?}", name, err);
            }
        }

        log::debug!("[{}] read_loop exited", name);
    }

    async fn write_loop(
        name: String,
        bytes_sent: Arc<AtomicUsize>,
        net_conn: Arc<dyn Conn + Send + Sync>,
        mut close_loop_ch: broadcast::Receiver<()>,
        association_internal: Arc<Mutex<AssociationInternal>>,
        awake_write_loop_ch: Arc<Notify>,
    ) {
        log::debug!("[{}] write_loop entered", name);
        let mut done = false;
        while !done {
            let (raw_packets, mut ok) = {
                let mut ai = association_internal.lock().await;
                ai.gather_outbound().await
            };

            for raw in &raw_packets {
                if let Err(err) = net_conn.send(raw).await {
                    log::warn!("[{}] failed to write packets on net_conn: {}", name, err);
                    ok = false;
                    break;
                } else {
                    bytes_sent.fetch_add(raw.len(), Ordering::SeqCst);
                }
            }

            if !ok {
                break;
            }

            tokio::select! {
                _ = awake_write_loop_ch.notified() =>{}
                _ = close_loop_ch.recv() => {
                    done = true;
                }
            };
        }

        {
            let mut ai = association_internal.lock().await;
            if let Err(err) = ai.close().await {
                log::warn!("[{}] failed to close association: {:?}", name, err);
            }
        }

        log::debug!("[{}] write_loop exited", name);
    }

    /// bytes_sent returns the number of bytes sent
    pub fn bytes_sent(&self) -> usize {
        self.bytes_sent.load(Ordering::SeqCst)
    }

    /// bytes_received returns the number of bytes received
    pub fn bytes_received(&self) -> usize {
        self.bytes_received.load(Ordering::SeqCst)
    }

    /// open_stream opens a stream
    pub async fn open_stream(
        &self,
        stream_identifier: u16,
        default_payload_type: PayloadProtocolIdentifier,
    ) -> Result<Arc<Stream>, Error> {
        let mut ai = self.association_internal.lock().await;
        ai.open_stream(stream_identifier, default_payload_type)
    }

    /// accept_stream accepts a stream
    pub async fn accept_stream(&mut self) -> Result<Arc<Stream>, Error> {
        if let Some(accept_ch) = &mut self.accept_ch_rx {
            if let Some(s) = accept_ch.recv().await {
                Ok(s)
            } else {
                Err(Error::ErrEof)
            }
        } else {
            Err(Error::ErrAssociationInitFailed)
        }
    }

    /// max_message_size returns the maximum message size you can send.
    pub fn max_message_size(&self) -> u32 {
        self.max_message_size.load(Ordering::SeqCst)
    }

    /// set_max_message_size sets the maximum message size you can send.
    pub fn set_max_message_size(&self, max_message_size: u32) {
        self.max_message_size
            .store(max_message_size, Ordering::SeqCst);
    }

    /// set_state atomically sets the state of the Association.
    fn set_state(&self, new_state: AssociationState) {
        let old_state = AssociationState::from(self.state.swap(new_state as u8, Ordering::SeqCst));
        if new_state != old_state {
            log::debug!(
                "[{}] state change: '{}' => '{}'",
                self.name,
                old_state,
                new_state,
            );
        }
    }

    /// get_state atomically returns the state of the Association.
    fn get_state(&self) -> AssociationState {
        self.state.load(Ordering::SeqCst).into()
    }
}
