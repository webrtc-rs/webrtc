#[cfg(test)]
mod association_test;

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
use crate::error::{Error, Result};
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

use bytes::{Bytes, BytesMut};
use rand::random;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{broadcast, mpsc, Mutex, Semaphore};
use util::Conn;

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
        write!(f, "{s}")
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
        write!(f, "{s}")
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
        write!(f, "{s}")
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
        write!(f, "{s}")
    }
}

/// Config collects the arguments to create_association construction into
/// a single structure
pub struct Config {
    pub net_conn: Arc<dyn Conn + Send + Sync>,
    pub max_receive_buffer_size: u32,
    pub max_message_size: u32,
    pub name: String,
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
    awake_write_loop_ch: Arc<mpsc::Sender<()>>,
    close_loop_ch_rx: Mutex<broadcast::Receiver<()>>,
    accept_ch_rx: Mutex<mpsc::Receiver<Arc<Stream>>>,
    net_conn: Arc<dyn Conn + Send + Sync>,
    bytes_received: Arc<AtomicUsize>,
    bytes_sent: Arc<AtomicUsize>,

    pub(crate) association_internal: Arc<Mutex<AssociationInternal>>,
}

impl Association {
    /// server accepts a SCTP stream over a conn
    pub async fn server(config: Config) -> Result<Self> {
        let (a, mut handshake_completed_ch_rx) = Association::new(config, false).await?;

        if let Some(err_opt) = handshake_completed_ch_rx.recv().await {
            if let Some(err) = err_opt {
                Err(err)
            } else {
                Ok(a)
            }
        } else {
            Err(Error::ErrAssociationHandshakeClosed)
        }
    }

    /// Client opens a SCTP stream over a conn
    pub async fn client(config: Config) -> Result<Self> {
        let (a, mut handshake_completed_ch_rx) = Association::new(config, true).await?;

        if let Some(err_opt) = handshake_completed_ch_rx.recv().await {
            if let Some(err) = err_opt {
                Err(err)
            } else {
                Ok(a)
            }
        } else {
            Err(Error::ErrAssociationHandshakeClosed)
        }
    }

    /// Shutdown initiates the shutdown sequence. The method blocks until the
    /// shutdown sequence is completed and the connection is closed, or until the
    /// passed context is done, in which case the context's error is returned.
    pub async fn shutdown(&self) -> Result<()> {
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
            let _ = self.awake_write_loop_ch.try_send(());
            self.set_state(AssociationState::ShutdownSent);
        }

        {
            let mut close_loop_ch_rx = self.close_loop_ch_rx.lock().await;
            let _ = close_loop_ch_rx.recv().await;
        }

        Ok(())
    }

    /// Close ends the SCTP Association and cleans up any state
    pub async fn close(&self) -> Result<()> {
        log::debug!("[{}] closing association..", self.name);

        let _ = self.net_conn.close().await;

        let mut ai = self.association_internal.lock().await;
        ai.close().await
    }

    async fn new(config: Config, is_client: bool) -> Result<(Self, mpsc::Receiver<Option<Error>>)> {
        let net_conn = Arc::clone(&config.net_conn);

        let (awake_write_loop_ch_tx, awake_write_loop_ch_rx) = mpsc::channel(1);
        let (accept_ch_tx, accept_ch_rx) = mpsc::channel(ACCEPT_CH_SIZE);
        let (handshake_completed_ch_tx, handshake_completed_ch_rx) = mpsc::channel(1);
        let (close_loop_ch_tx, close_loop_ch_rx) = broadcast::channel(1);
        let (close_loop_ch_rx1, close_loop_ch_rx2) =
            (close_loop_ch_tx.subscribe(), close_loop_ch_tx.subscribe());
        let awake_write_loop_ch = Arc::new(awake_write_loop_ch_tx);

        let ai = AssociationInternal::new(
            config,
            close_loop_ch_tx,
            accept_ch_tx,
            handshake_completed_ch_tx,
            Arc::clone(&awake_write_loop_ch),
        );

        let bytes_received = Arc::new(AtomicUsize::new(0));
        let bytes_sent = Arc::new(AtomicUsize::new(0));
        let name = ai.name.clone();
        let state = Arc::clone(&ai.state);
        let max_message_size = Arc::clone(&ai.max_message_size);
        let inflight_queue_length = Arc::clone(&ai.inflight_queue_length);
        let will_send_shutdown = Arc::clone(&ai.will_send_shutdown);

        let mut init = ChunkInit {
            initial_tsn: ai.my_next_tsn,
            num_outbound_streams: ai.my_max_num_outbound_streams,
            num_inbound_streams: ai.my_max_num_inbound_streams,
            initiate_tag: ai.my_verification_tag,
            advertised_receiver_window_credit: ai.max_receive_buffer_size,
            ..Default::default()
        };
        init.set_supported_extensions();

        let name1 = name.clone();
        let name2 = name.clone();

        let bytes_received1 = Arc::clone(&bytes_received);
        let bytes_sent2 = Arc::clone(&bytes_sent);

        let net_conn1 = Arc::clone(&net_conn);
        let net_conn2 = Arc::clone(&net_conn);

        let association_internal = Arc::new(Mutex::new(ai));
        let association_internal1 = Arc::clone(&association_internal);
        let association_internal2 = Arc::clone(&association_internal);

        {
            let association_internal3 = Arc::clone(&association_internal);

            let mut ai = association_internal.lock().await;
            ai.t1init = Some(RtxTimer::new(
                Arc::downgrade(&association_internal3),
                RtxTimerId::T1Init,
                MAX_INIT_RETRANS,
            ));
            ai.t1cookie = Some(RtxTimer::new(
                Arc::downgrade(&association_internal3),
                RtxTimerId::T1Cookie,
                MAX_INIT_RETRANS,
            ));
            ai.t2shutdown = Some(RtxTimer::new(
                Arc::downgrade(&association_internal3),
                RtxTimerId::T2Shutdown,
                NO_MAX_RETRANS,
            )); // retransmit forever
            ai.t3rtx = Some(RtxTimer::new(
                Arc::downgrade(&association_internal3),
                RtxTimerId::T3RTX,
                NO_MAX_RETRANS,
            )); // retransmit forever
            ai.treconfig = Some(RtxTimer::new(
                Arc::downgrade(&association_internal3),
                RtxTimerId::Reconfig,
                NO_MAX_RETRANS,
            )); // retransmit forever
            ai.ack_timer = Some(AckTimer::new(
                Arc::downgrade(&association_internal3),
                ACK_INTERVAL,
            ));
        }

        tokio::spawn(async move {
            Association::read_loop(
                name1,
                bytes_received1,
                net_conn1,
                close_loop_ch_rx1,
                association_internal1,
            )
            .await;
        });

        tokio::spawn(async move {
            Association::write_loop(
                name2,
                bytes_sent2,
                net_conn2,
                close_loop_ch_rx2,
                association_internal2,
                awake_write_loop_ch_rx,
            )
            .await;
        });

        if is_client {
            let mut ai = association_internal.lock().await;
            ai.set_state(AssociationState::CookieWait);
            ai.stored_init = Some(init);
            ai.send_init()?;
            let rto = ai.rto_mgr.get_rto();
            if let Some(t1init) = &ai.t1init {
                t1init.start(rto).await;
            }
        }

        Ok((
            Association {
                name,
                state,
                max_message_size,
                inflight_queue_length,
                will_send_shutdown,
                awake_write_loop_ch,
                close_loop_ch_rx: Mutex::new(close_loop_ch_rx),
                accept_ch_rx: Mutex::new(accept_ch_rx),
                net_conn,
                bytes_received,
                bytes_sent,
                association_internal,
            },
            handshake_completed_ch_rx,
        ))
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
            log::debug!("[{}] recving {} bytes", name, n);
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
        mut awake_write_loop_ch: mpsc::Receiver<()>,
    ) {
        log::debug!("[{}] write_loop entered", name);
        let done = Arc::new(AtomicBool::new(false));
        let name = Arc::new(name);

        let limit = {
            #[cfg(test)]
            {
                1
            }
            #[cfg(not(test))]
            {
                8
            }
        };

        let sem = Arc::new(Semaphore::new(limit));
        while !done.load(Ordering::Relaxed) {
            //log::debug!("[{}] gather_outbound begin", name);
            let (packets, continue_loop) = {
                let mut ai = association_internal.lock().await;
                ai.gather_outbound().await
            };
            //log::debug!("[{}] gather_outbound done with {}", name, packets.len());

            // We schedule a new task here for a reason:
            // If we don't tokio tends to run the write_loop and read_loop of one connection on the same OS thread
            // This means that even though we release the lock above, the read_loop isn't able to take it, simply because it is not being scheduled by tokio
            // Doing it this way, tokio schedules this to a new thread, this future is suspended, and the read_loop can make progress
            let net_conn = Arc::clone(&net_conn);
            let bytes_sent = Arc::clone(&bytes_sent);
            let name2 = Arc::clone(&name);
            let done2 = Arc::clone(&done);
            let sem = Arc::clone(&sem);
            sem.acquire().await.unwrap().forget();
            tokio::task::spawn(async move {
                let mut buf = BytesMut::with_capacity(16 * 1024);
                for raw in packets {
                    buf.clear();
                    if let Err(err) = raw.marshal_to(&mut buf) {
                        log::warn!("[{}] failed to serialize a packet: {:?}", name2, err);
                    } else {
                        let raw = buf.as_ref();
                        if let Err(err) = net_conn.send(raw.as_ref()).await {
                            log::warn!("[{}] failed to write packets on net_conn: {}", name2, err);
                            done2.store(true, Ordering::Relaxed)
                        } else {
                            bytes_sent.fetch_add(raw.len(), Ordering::SeqCst);
                        }
                    }
                    //log::debug!("[{}] sending {} bytes done", name, raw.len());
                }
                sem.add_permits(1);
            });

            if !continue_loop {
                break;
            }

            //log::debug!("[{}] wait awake_write_loop_ch", name);
            tokio::select! {
                _ = awake_write_loop_ch.recv() =>{}
                _ = close_loop_ch.recv() => {
                    done.store(true, Ordering::Relaxed);
                }
            };
            //log::debug!("[{}] wait awake_write_loop_ch done", name);
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
    ) -> Result<Arc<Stream>> {
        let mut ai = self.association_internal.lock().await;
        ai.open_stream(stream_identifier, default_payload_type)
    }

    /// accept_stream accepts a stream
    pub async fn accept_stream(&self) -> Option<Arc<Stream>> {
        let mut accept_ch_rx = self.accept_ch_rx.lock().await;
        accept_ch_rx.recv().await
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
