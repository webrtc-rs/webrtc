use crate::association_stats::AssociationStats;
use crate::chunk::chunk_cookie_echo::ChunkCookieEcho;
use crate::chunk::chunk_init::ChunkInit;
use crate::chunk::chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier};
use crate::chunk::chunk_reconfig::ChunkReconfig;
use crate::chunk::chunk_selective_ack::ChunkSelectiveAck;
use crate::chunk::chunk_shutdown::ChunkShutdown;
use crate::chunk::chunk_shutdown_ack::ChunkShutdownAck;
use crate::chunk::chunk_shutdown_complete::ChunkShutdownComplete;
use crate::chunk::Chunk;
use crate::error::Error;
use crate::error_cause::*;
use crate::packet::Packet;
use crate::param::param_outgoing_reset_request::ParamOutgoingResetRequest;
use crate::param::param_reconfig_response::{ParamReconfigResponse, ReconfigResult};
use crate::param::param_state_cookie::ParamStateCookie;
use crate::queue::control_queue::ControlQueue;
use crate::queue::payload_queue::PayloadQueue;
use crate::queue::pending_queue::PendingQueue;
use crate::stream::{ReliabilityType, Stream};
use crate::timer::ack_timer::{AckTimer, ACK_INTERVAL};
use crate::timer::rtx_timer::{RtoManager, RtxTimer, MAX_INIT_RETRANS, NO_MAX_RETRANS};
use crate::util::*;

use util::Conn;
//use async_trait::async_trait;
use crate::chunk::chunk_error::ChunkError;
use crate::chunk::chunk_forward_tsn::{ChunkForwardTsn, ChunkForwardTsnStream};
use crate::param::Param;
use bytes::Bytes;
use rand::random;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::Notify;

pub(crate) const RECEIVE_MTU: u32 = 8192;
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
///Peer        : Tag value to be sent in every packet and is received
///Verification: in the INIT or INIT ACK chunk.
///Tag         :
//
///My          : Tag expected in every inbound packet and sent in the
///Verification: INIT or INIT ACK chunk.
//
///Tag         :
///State       : A state variable indicating what state the association
///            : is in, i.e., COOKIE-WAIT, COOKIE-ECHOED, ESTABLISHED,
///            : SHUTDOWN-PENDING, SHUTDOWN-SENT, SHUTDOWN-RECEIVED,
///            : SHUTDOWN-ACK-SENT.
//
///              Note: No "CLOSED" state is illustrated since if a
///              association is "CLOSED" its TCB SHOULD be removed.
#[derive(Default)]
pub struct Association {
    bytes_received: u64,
    bytes_sent: u64,

    //lock sync.RWMutex
    net_conn: Option<Arc<dyn Conn + Send + Sync>>,

    peer_verification_tag: u32,
    my_verification_tag: u32,
    state: Arc<AtomicU8>,
    my_next_tsn: u32,         // nextTSN
    peer_last_tsn: u32,       // lastRcvdTSN
    min_tsn2measure_rtt: u32, // for RTT measurement
    will_send_forward_tsn: bool,
    will_retransmit_fast: bool,
    will_retransmit_reconfig: bool,

    will_send_shutdown: bool,
    will_send_shutdown_ack: bool,
    will_send_shutdown_complete: bool,

    // Reconfig
    my_next_rsn: u32,
    reconfigs: HashMap<u32, ChunkReconfig>,
    reconfig_requests: HashMap<u32, ParamOutgoingResetRequest>,

    // Non-RFC internal data
    source_port: u16,
    destination_port: u16,
    my_max_num_inbound_streams: u16,
    my_max_num_outbound_streams: u16,
    my_cookie: ParamStateCookie,
    payload_queue: PayloadQueue,
    inflight_queue: PayloadQueue,
    pending_queue: PendingQueue,
    control_queue: ControlQueue,
    mtu: u32,
    max_payload_size: u32, // max DATA chunk payload size
    cumulative_tsnack_point: u32,
    advanced_peer_tsnack_point: u32,
    use_forward_tsn: bool,

    // Congestion control parameters
    max_receive_buffer_size: u32,
    max_message_size: Arc<AtomicU32>,
    cwnd: u32,     // my congestion window size
    rwnd: u32,     // calculated peer's receiver windows size
    ssthresh: u32, // slow start threshold
    partial_bytes_acked: u32,
    in_fast_recovery: bool,
    fast_recover_exit_point: u32,

    // RTX & Ack timer
    rto_mgr: RtoManager,
    t1init: RtxTimer,
    t1cookie: RtxTimer,
    t2shutdown: RtxTimer,
    t3rtx: RtxTimer,
    treconfig: RtxTimer,
    ack_timer: AckTimer,

    // Chunks stored for retransmission
    stored_init: Option<ChunkInit>,
    stored_cookie_echo: Option<ChunkCookieEcho>,

    streams: HashMap<u16, Stream>,
    /*TODO:     acceptCh             chan *Stream
        readLoopCloseCh      chan struct{}

        closeWriteLoopCh     chan struct{}

    */
    awake_write_loop_ch: Notify,

    //TODO: handshakeCompletedCh : mpsc:: chan error
    //TODO: closeWriteLoopOnce sync.Once

    // local error
    silent_error: Option<Error>,

    ack_state: AckState,
    ack_mode: AckMode, // for testing

    // stats
    stats: AssociationStats,

    // per inbound packet context
    delayed_ack_triggered: bool,
    immediate_ack_triggered: bool,

    name: String,
    //log  logging.LeveledLogger
}

impl Association {
    /*/// Server accepts a SCTP stream over a conn
    pub fn Server(config: Config) ->Result<Self, Error> {
        a := create_association(config)
        a.init(false)

        select {
        case err := <-a.handshakeCompletedCh:
            if err != nil {
                return nil, err
            }
            return a, nil
        case <-a.readLoopCloseCh:
            return nil, errAssociationClosedBeforeConn
        }
    }

    /// Client opens a SCTP stream over a conn
    func Client(config Config) (*Association, error) {
        a := create_association(config)
        a.init(true)

        select {
        case err := <-a.handshakeCompletedCh:
            if err != nil {
                return nil, err
            }
            return a, nil
        case <-a.readLoopCloseCh:
            return nil, errAssociationClosedBeforeConn
        }
    }*/

    fn create_association(config: Config) -> Self {
        let max_receive_buffer_size = if config.max_receive_buffer_size == 0 {
            INITIAL_RECV_BUF_SIZE
        } else {
            config.max_receive_buffer_size
        };

        let max_message_size = if config.max_message_size == 0 {
            DEFAULT_MAX_MESSAGE_SIZE
        } else {
            config.max_message_size
        };

        let tsn = random::<u32>();
        let mut a = Association {
            net_conn: Some(config.net_conn),
            max_receive_buffer_size,
            max_message_size: Arc::new(AtomicU32::new(max_message_size)),
            my_max_num_outbound_streams: u16::MAX,
            my_max_num_inbound_streams: u16::MAX,
            payload_queue: PayloadQueue::new(),
            inflight_queue: PayloadQueue::new(),
            pending_queue: PendingQueue::new(),
            control_queue: ControlQueue::new(),
            mtu: INITIAL_MTU,
            max_payload_size: INITIAL_MTU - (COMMON_HEADER_SIZE + DATA_CHUNK_HEADER_SIZE),
            my_verification_tag: random::<u32>(),
            my_next_tsn: tsn,
            my_next_rsn: tsn,
            min_tsn2measure_rtt: tsn,
            state: Arc::new(AtomicU8::new(AssociationState::Closed as u8)),
            rto_mgr: RtoManager::new(),
            streams: HashMap::new(),
            reconfigs: HashMap::new(),
            reconfig_requests: HashMap::new(),
            /*acceptCh:                make(chan *Stream, ACCEPT_CH_SIZE),
            readLoopCloseCh:         make(chan struct{}),
            awake_write_loop_ch:        make(chan struct{}, 1),
            closeWriteLoopCh:        make(chan struct{}),
            handshakeCompletedCh:    make(chan error),*/
            cumulative_tsnack_point: tsn - 1,
            advanced_peer_tsnack_point: tsn - 1,
            silent_error: Some(Error::ErrSilentlyDiscard),
            stats: AssociationStats::default(),
            //log:                     config.LoggerFactory.NewLogger("sctp"),
            ..Default::default()
        };

        a.name = format!("{:p}", &a);

        // RFC 4690 Sec 7.2.1
        //  o  The initial cwnd before DATA transmission or after a sufficiently
        //     long idle period MUST be set to min(4*MTU, max (2*MTU, 4380
        //     bytes)).
        a.cwnd = std::cmp::min(4 * a.mtu, std::cmp::max(2 * a.mtu, 4380));
        log::trace!(
            "[{}] updated cwnd={} ssthresh={} inflight={} (INI)",
            a.name,
            a.cwnd,
            a.ssthresh,
            a.inflight_queue.get_num_bytes()
        );

        a.t1init = RtxTimer::new(RtxTimerId::T1Init, MAX_INIT_RETRANS);
        a.t1cookie = RtxTimer::new(RtxTimerId::T1Cookie, MAX_INIT_RETRANS);
        a.t2shutdown = RtxTimer::new(RtxTimerId::T2Shutdown, NO_MAX_RETRANS); // retransmit forever
        a.t3rtx = RtxTimer::new(RtxTimerId::T3RTX, NO_MAX_RETRANS); // retransmit forever
        a.treconfig = RtxTimer::new(RtxTimerId::Reconfig, NO_MAX_RETRANS); // retransmit forever
        a.ack_timer = AckTimer::new(ACK_INTERVAL);

        a
    }

    /*
            fn init(&self, isClient: bool) {

                 //TODO: go a.readLoop()
                 //TODO: go a.writeLoop()

                 if isClient {
                     a.set_state(CookieWait)
                     init := &chunkInit{}
                     init.initialTSN = a.my_next_tsn
                     init.numOutboundStreams = a.my_max_num_outbound_streams
                     init.numInboundStreams = a.my_max_num_inbound_streams
                     init.initiateTag = a.my_verification_tag
                     init.advertisedReceiverWindowCredit = a.max_receive_buffer_size
                     setSupportedExtensions(&init.chunkInitCommon)
                     a.stored_init = init

                     err := a.sendInit()
                     if err != nil {
                         a.log.Errorf("[%s] failed to send init: %s", a.name, err.Error())
                     }

                     a.t1init.start(a.rto_mgr.getRTO())
                 }
             }
    */

    /// caller must hold a.lock
    fn send_init(&mut self) -> Result<(), Error> {
        if let Some(stored_init) = &self.stored_init {
            log::debug!("[{}] sending INIT", self.name);

            self.source_port = 5000; // Spec??
            self.destination_port = 5000; // Spec??

            let outbound = Packet {
                source_port: self.source_port,
                destination_port: self.destination_port,
                verification_tag: self.peer_verification_tag,
                chunks: vec![Box::new(stored_init.clone())],
            };

            self.control_queue.push_back(outbound);
            self.awake_write_loop();

            Ok(())
        } else {
            Err(Error::ErrInitNotStoredToSend)
        }
    }

    /// caller must hold a.lock
    fn send_cookie_echo(&mut self) -> Result<(), Error> {
        if let Some(stored_cookie_echo) = &self.stored_cookie_echo {
            log::debug!("[{}] sending COOKIE-ECHO", self.name);

            let outbound = Packet {
                source_port: self.source_port,
                destination_port: self.destination_port,
                verification_tag: self.peer_verification_tag,
                chunks: vec![Box::new(stored_cookie_echo.clone())],
            };

            self.control_queue.push_back(outbound);
            self.awake_write_loop();
            Ok(())
        } else {
            Err(Error::ErrCookieEchoNotStoredToSend)
        }
    }
    /*
    // Shutdown initiates the shutdown sequence. The method blocks until the
    // shutdown sequence is completed and the connection is closed, or until the
    // passed context is done, in which case the context's error is returned.
    fn Shutdown(ctx context.Context) error {
        a.log.Debugf("[%s] closing association..", a.name)

        state := a.get_state()

        if state != Established {
            return fmt.Errorf("%w: shutdown %s", errShutdownNonEstablished, a.name)
        }

        // Attempt a graceful shutdown.
        a.set_state(ShutdownPending)

        a.lock.Lock()

        if a.inflight_queue.size() == 0 {
            // No more outstanding, send shutdown.
            a.will_send_shutdown = true
            a.awake_write_loop()
            a.set_state(ShutdownSent)
        }

        a.lock.Unlock()

        select {
        case <-a.closeWriteLoopCh:
            return nil
        case <-ctx.Done():
            return ctx.Err()
        }
    }

    // Close ends the SCTP Association and cleans up any state
    fn Close() error {
        a.log.Debugf("[%s] closing association..", a.name)

        err := a.close()

        // Wait for readLoop to end
        <-a.readLoopCloseCh

        a.log.Debugf("[%s] association closed", a.name)
        a.log.Debugf("[%s] stats nDATAs (in) : %d", a.name, a.stats.get_num_datas())
        a.log.Debugf("[%s] stats nSACKs (in) : %d", a.name, a.stats.get_num_sacks())
        a.log.Debugf("[%s] stats nT3Timeouts : %d", a.name, a.stats.get_num_t3timeouts())
        a.log.Debugf("[%s] stats nAckTimeouts: %d", a.name, a.stats.get_num_ack_timeouts())
        a.log.Debugf("[%s] stats nFastRetrans: %d", a.name, a.stats.get_num_fast_retrans())

        return err
    }

    fn close() error {
        a.log.Debugf("[%s] closing association..", a.name)

        a.set_state(closed)

        err := a.net_conn.Close()

        a.closeAllTimers()

        // awake writeLoop to exit
        a.closeWriteLoopOnce.Do(func() { close(a.closeWriteLoopCh) })

        return err
    }

    fn closeAllTimers() {
        // Close all retransmission & ack timers
        a.t1init.close()
        a.t1cookie.close()
        a.t2shutdown.close()
        a.t3rtx.close()
        a.t_reconfig.close()
        a.ack_timer.close()
    }

    fn readLoop() {
        var closeErr error
        defer func() {
            // also stop writeLoop, otherwise writeLoop can be leaked
            // if connection is lost when there is no writing packet.
            a.closeWriteLoopOnce.Do(func() { close(a.closeWriteLoopCh) })

            a.lock.Lock()
            for _, s := range a.streams {
                a.unregister_stream(s, closeErr)
            }
            a.lock.Unlock()
            close(a.acceptCh)
            close(a.readLoopCloseCh)

            a.log.Debugf("[%s] association closed", a.name)
            a.log.Debugf("[%s] stats nDATAs (in) : %d", a.name, a.stats.get_num_datas())
            a.log.Debugf("[%s] stats nSACKs (in) : %d", a.name, a.stats.get_num_sacks())
            a.log.Debugf("[%s] stats nT3Timeouts : %d", a.name, a.stats.get_num_t3timeouts())
            a.log.Debugf("[%s] stats nAckTimeouts: %d", a.name, a.stats.get_num_ack_timeouts())
            a.log.Debugf("[%s] stats nFastRetrans: %d", a.name, a.stats.get_num_fast_retrans())
        }()

        a.log.Debugf("[%s] readLoop entered", a.name)
        buffer := make([]byte, RECEIVE_MTU)

        for {
            n, err := a.net_conn.read(buffer)
            if err != nil {
                closeErr = err
                break
            }
            // Make a buffer sized to what we read, then copy the data we
            // read from the underlying transport. We do this because the
            // user data is passed to the reassembly queue without
            // copying.
            inbound := make([]byte, n)
            copy(inbound, buffer[:n])
            atomic.AddUint64(&a.bytes_received, uint64(n))
            if err = a.handleInbound(inbound); err != nil {
                closeErr = err
                break
            }
        }

        a.log.Debugf("[%s] readLoop exited %s", a.name, closeErr)
    }

    fn writeLoop() {
        a.log.Debugf("[%s] writeLoop entered", a.name)
        defer a.log.Debugf("[%s] writeLoop exited", a.name)

    loop:
        for {
            rawPackets, ok := a.gather_outbound()

            for _, raw := range rawPackets {
                _, err := a.net_conn.write(raw)
                if err != nil {
                    if err != io.EOF {
                        a.log.Warnf("[%s] failed to write packets on net_conn: %v", a.name, err)
                    }
                    a.log.Debugf("[%s] writeLoop ended", a.name)
                    break loop
                }
                atomic.AddUint64(&a.bytes_sent, uint64(len(raw)))
            }

            if !ok {
                if err := a.close(); err != nil {
                    a.log.Warnf("[%s] failed to close association: %v", a.name, err)
                }

                return
            }

            select {
            case <-a.awake_write_loop_ch:
            case <-a.closeWriteLoopCh:
                break loop
            }
        }

        a.set_state(closed)
        a.closeAllTimers()
    }*/

    fn awake_write_loop(&self) {
        self.awake_write_loop_ch.notify_one();
    }

    /// unregister_stream un-registers a stream from the association
    /// The caller should hold the association write lock.
    fn unregister_stream(&mut self, stream_identifier: u16, _err: Error) {
        let s = self.streams.remove(&stream_identifier);
        if let Some(s) = s {
            //TODO: s.readErr = err
            s.read_notifier.notify_waiters();
        }
    }
    /*
                                  // handleInbound parses incoming raw packets
                                  fn handleInbound(raw []byte) error {
                                      p := &packet{}
                                      if err := p.unmarshal(raw); err != nil {
                                          a.log.Warnf("[%s] unable to parse SCTP packet %s", a.name, err)
                                          return nil
                                      }

                                      if err := check_packet(p); err != nil {
                                          a.log.Warnf("[%s] failed validating packet %s", a.name, err)
                                          return nil
                                      }

                                      a.handle_chunk_start()

                                      for _, c := range p.chunks {
                                          if err := a.handleChunk(p, c); err != nil {
                                              return err
                                          }
                                      }

                                      a.handleChunkEnd()

                                      return nil
                                  }

                                  // The caller should hold the lock
                                  fn gatherDataPacketsToRetransmit(rawPackets [][]byte) [][]byte {
                                      for _, p := range a.get_data_packets_to_retransmit() {
                                          raw, err := p.marshal()
                                          if err != nil {
                                              a.log.Warnf("[%s] failed to serialize a DATA packet to be retransmitted", a.name)
                                              continue
                                          }
                                          rawPackets = append(rawPackets, raw)
                                      }

                                      return rawPackets
                                  }

                                  // The caller should hold the lock
                                  fn gatherOutboundDataAndReconfigPackets(rawPackets [][]byte) [][]byte {
                                      // Pop unsent data chunks from the pending queue to send as much as
                                      // cwnd and rwnd allow.
                                      chunks, sisToReset := a.pop_pending_data_chunks_to_send()
                                      if len(chunks) > 0 {
                                          // Start timer. (noop if already started)
                                          a.log.Tracef("[%s] T3-rtx timer start (pt1)", a.name)
                                          a.t3rtx.start(a.rto_mgr.getRTO())
                                          for _, p := range a.bundle_data_chunks_into_packets(chunks) {
                                              raw, err := p.marshal()
                                              if err != nil {
                                                  a.log.Warnf("[%s] failed to serialize a DATA packet", a.name)
                                                  continue
                                              }
                                              rawPackets = append(rawPackets, raw)
                                          }
                                      }

                                      if len(sisToReset) > 0 || a.will_retransmit_reconfig {
                                          if a.will_retransmit_reconfig {
                                              a.will_retransmit_reconfig = false
                                              a.log.Debugf("[%s] retransmit %d RECONFIG chunk(s)", a.name, len(a.reconfigs))
                                              for _, c := range a.reconfigs {
                                                  p := a.create_packet([]chunk{c})
                                                  raw, err := p.marshal()
                                                  if err != nil {
                                                      a.log.Warnf("[%s] failed to serialize a RECONFIG packet to be retransmitted", a.name)
                                                  } else {
                                                      rawPackets = append(rawPackets, raw)
                                                  }
                                              }
                                          }

                                          if len(sisToReset) > 0 {
                                              rsn := a.generate_next_rsn()
                                              tsn := a.my_next_tsn - 1
                                              c := &chunkReconfig{
                                                  paramA: &paramOutgoingResetRequest{
                                                      reconfigRequestSequenceNumber: rsn,
                                                      senderLastTSN:                 tsn,
                                                      streamIdentifiers:             sisToReset,
                                                  },
                                              }
                                              a.reconfigs[rsn] = c // store in the map for retransmission
                                              a.log.Debugf("[%s] sending RECONFIG: rsn=%d tsn=%d streams=%v",
                                                  a.name, rsn, a.my_next_tsn-1, sisToReset)
                                              p := a.create_packet([]chunk{c})
                                              raw, err := p.marshal()
                                              if err != nil {
                                                  a.log.Warnf("[%s] failed to serialize a RECONFIG packet to be transmitted", a.name)
                                              } else {
                                                  rawPackets = append(rawPackets, raw)
                                              }
                                          }

                                          if len(a.reconfigs) > 0 {
                                              a.t_reconfig.start(a.rto_mgr.getRTO())
                                          }
                                      }

                                      return rawPackets
                                  }

                                  // The caller should hold the lock
                                  fn gatherOutboundFastRetransmissionPackets(rawPackets [][]byte) [][]byte {
                                      if a.will_retransmit_fast {
                                          a.will_retransmit_fast = false

                                          toFastRetrans := []chunk{}
                                          fastRetransSize := COMMON_HEADER_SIZE

                                          for i := 0; ; i++ {
                                              c, ok := a.inflight_queue.get(a.cumulative_tsnack_point + uint32(i) + 1)
                                              if !ok {
                                                  break // end of pending data
                                              }

                                              if c.acked || c.abandoned() {
                                                  continue
                                              }

                                              if c.nSent > 1 || c.missIndicator < 3 {
                                                  continue
                                              }

                                              // RFC 4960 Sec 7.2.4 Fast Retransmit on Gap Reports
                                              //  3)  Determine how many of the earliest (i.e., lowest TSN) DATA chunks
                                              //      marked for retransmission will fit into a single packet, subject
                                              //      to constraint of the path MTU of the destination transport
                                              //      address to which the packet is being sent.  Call this value K.
                                              //      Retransmit those K DATA chunks in a single packet.  When a Fast
                                              //      Retransmit is being performed, the sender SHOULD ignore the value
                                              //      of cwnd and SHOULD NOT delay retransmission for this single
                                              //		packet.

                                              dataChunkSize := DATA_CHUNK_HEADER_SIZE + uint32(len(c.userData))
                                              if a.mtu < fastRetransSize+dataChunkSize {
                                                  break
                                              }

                                              fastRetransSize += dataChunkSize
                                              a.stats.inc_fast_retrans()
                                              c.nSent++
                                              a.check_partial_reliability_status(c)
                                              toFastRetrans = append(toFastRetrans, c)
                                              a.log.Tracef("[%s] fast-retransmit: tsn=%d sent=%d htna=%d",
                                                  a.name, c.tsn, c.nSent, a.fast_recover_exit_point)
                                          }

                                          if len(toFastRetrans) > 0 {
                                              raw, err := a.create_packet(toFastRetrans).marshal()
                                              if err != nil {
                                                  a.log.Warnf("[%s] failed to serialize a DATA packet to be fast-retransmitted", a.name)
                                              } else {
                                                  rawPackets = append(rawPackets, raw)
                                              }
                                          }
                                      }

                                      return rawPackets
                                  }

                                  // The caller should hold the lock
                                  fn gatherOutboundSackPackets(rawPackets [][]byte) [][]byte {
                                      if a.ack_state == ackStateImmediate {
                                          a.ack_state = ackStateIdle
                                          sack := a.create_selective_ack_chunk()
                                          a.log.Debugf("[%s] sending SACK: %s", a.name, sack.String())
                                          raw, err := a.create_packet([]chunk{sack}).marshal()
                                          if err != nil {
                                              a.log.Warnf("[%s] failed to serialize a SACK packet", a.name)
                                          } else {
                                              rawPackets = append(rawPackets, raw)
                                          }
                                      }

                                      return rawPackets
                                  }
    */
    /// The caller should hold the lock
    fn gather_outbound_forward_tsn_packets(&mut self, mut raw_packets: Vec<Bytes>) -> Vec<Bytes> {
        if self.will_send_forward_tsn {
            self.will_send_forward_tsn = false;
            if sna32gt(
                self.advanced_peer_tsnack_point,
                self.cumulative_tsnack_point,
            ) {
                let fwd_tsn = self.create_forward_tsn();
                if let Ok(raw) = self.create_packet(vec![Box::new(fwd_tsn)]).marshal() {
                    raw_packets.push(raw);
                } else {
                    log::warn!("[{}] failed to serialize a Forward TSN packet", self.name);
                }
            }
        }

        raw_packets
    }

    fn gather_outbound_shutdown_packets(
        &mut self,
        mut raw_packets: Vec<Bytes>,
    ) -> (Vec<Bytes>, bool) {
        let mut ok = true;

        if self.will_send_shutdown {
            self.will_send_shutdown = false;

            let shutdown = ChunkShutdown {
                cumulative_tsn_ack: self.cumulative_tsnack_point,
            };

            if let Ok(raw) = self.create_packet(vec![Box::new(shutdown)]).marshal() {
                //TODO: add observer: self.t2shutdown.start(self.rto_mgr.get_rto());
                raw_packets.push(raw);
            } else {
                log::warn!("[{}] failed to serialize a Shutdown packet", self.name);
            }
        } else if self.will_send_shutdown_ack {
            self.will_send_shutdown_ack = false;

            let shutdown_ack = ChunkShutdownAck {};

            if let Ok(raw) = self.create_packet(vec![Box::new(shutdown_ack)]).marshal() {
                //TODO: add observer: self.t2shutdown.start(self.rto_mgr.get_rto());
                raw_packets.push(raw);
            } else {
                log::warn!("[{}] failed to serialize a ShutdownAck packet", self.name);
            }
        } else if self.will_send_shutdown_complete {
            self.will_send_shutdown_complete = false;

            let shutdown_complete = ChunkShutdownComplete {};

            if let Ok(raw) = self
                .create_packet(vec![Box::new(shutdown_complete)])
                .marshal()
            {
                raw_packets.push(raw);
                ok = false;
            } else {
                log::warn!(
                    "[{}] failed to serialize a ShutdownComplete packet",
                    self.name
                );
            }
        }

        (raw_packets, ok)
    }

    /// gather_outbound gathers outgoing packets. The returned bool value set to
    /// false means the association should be closed down after the final send.
    fn gather_outbound(&mut self) -> (Vec<Bytes>, bool) {
        let mut raw_packets = vec![];

        if !self.control_queue.is_empty() {
            for p in self.control_queue.drain(..) {
                if let Ok(raw) = p.marshal() {
                    raw_packets.push(raw);
                } else {
                    log::warn!("[{}] failed to serialize a control packet", self.name);
                    continue;
                }
            }
        }

        let ok = true;

        /*TODO:
        let state = self.get_state();
           match state {
            AssociationState::Established=> {
                raw_packets = a.gatherDataPacketsToRetransmit(raw_packets)
                raw_packets = a.gatherOutboundDataAndReconfigPackets(raw_packets)
                raw_packets = a.gatherOutboundFastRetransmissionPackets(raw_packets)
                raw_packets = a.gatherOutboundSackPackets(raw_packets)
                raw_packets = a.gather_outbound_forward_tsnpackets(raw_packets)
            }
            AssociationState::ShutdownPending|
            AssociationState::ShutdownSent|
            AssociationState::ShutdownReceived => {
                raw_packets = a.gatherDataPacketsToRetransmit(raw_packets)
                raw_packets = a.gatherOutboundFastRetransmissionPackets(raw_packets)
                raw_packets = a.gatherOutboundSackPackets(raw_packets)
                raw_packets, ok = a.gather_outbound_shutdown_packets(raw_packets)
            }
            AssociationState::ShutdownAckSent => {
                raw_packets, ok = a.gather_outbound_shutdown_packets(raw_packets)
            }
            _=>{}
        };*/

        (raw_packets, ok)
    }

    fn check_packet(p: &Packet) -> Result<(), Error> {
        // All packets must adhere to these rules

        // This is the SCTP sender's port number.  It can be used by the
        // receiver in combination with the source IP address, the SCTP
        // destination port, and possibly the destination IP address to
        // identify the association to which this packet belongs.  The port
        // number 0 MUST NOT be used.
        if p.source_port == 0 {
            return Err(Error::ErrSctpPacketSourcePortZero);
        }

        // This is the SCTP port number to which this packet is destined.
        // The receiving host will use this port number to de-multiplex the
        // SCTP packet to the correct receiving endpoint/application.  The
        // port number 0 MUST NOT be used.
        if p.destination_port == 0 {
            return Err(Error::ErrSctpPacketDestinationPortZero);
        }

        // Check values on the packet that are specific to a particular chunk type
        for c in &p.chunks {
            if c.as_any().downcast_ref::<ChunkInit>().is_some() {
                // An INIT or INIT ACK chunk MUST NOT be bundled with any other chunk.
                // They MUST be the only chunks present in the SCTP packets that carry
                // them.
                if p.chunks.len() != 1 {
                    return Err(Error::ErrInitChunkBundled);
                }

                // A packet containing an INIT chunk MUST have a zero Verification
                // Tag.
                if p.verification_tag != 0 {
                    return Err(Error::ErrInitChunkVerifyTagNotZero);
                }
            }
        }

        Ok(())
    }

    /// set_state atomically sets the state of the Association.
    /// The caller should hold the lock.
    fn set_state(&self, new_state: u8) {
        let old_state = self.state.swap(new_state, Ordering::SeqCst);
        if new_state != old_state {
            log::debug!(
                "[{}] state change: '{}' => '{}'",
                self.name,
                AssociationState::from(old_state),
                AssociationState::from(new_state),
            );
        }
    }

    /// get_state atomically returns the state of the Association.
    fn get_state(&self) -> AssociationState {
        self.state.load(Ordering::SeqCst).into()
    }

    /// bytes_sent returns the number of bytes sent
    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent
        //return atomic.LoadUint64(&a.bytes_sent)
    }

    /// bytes_received returns the number of bytes received
    pub fn bytes_received(&self) -> u64 {
        self.bytes_received
        //return atomic.LoadUint64(&a.bytes_received)
    }
    /*
                                 func setSupportedExtensions(init *chunkInitCommon) {
                                     // nolint:godox
                                     // TODO RFC5061 https://tools.ietf.org/html/rfc6525#section-5.2
                                     // An implementation supporting this (Supported Extensions Parameter)
                                     // extension MUST list the ASCONF, the ASCONF-ACK, and the AUTH chunks
                                     // in its INIT and INIT-ACK parameters.
                                     init.params = append(init.params, &paramSupportedExtensions{
                                         ChunkTypes: []chunkType{ctReconfig, ctForwardTSN},
                                     })
                                 }

                                 // The caller should hold the lock.
                                 fn handleInit(p *packet, i *chunkInit) ([]*packet, error) {
                                     state := a.get_state()
                                     a.log.Debugf("[%s] chunkInit received in state '%s'", a.name, getAssociationStateString(state))

                                     // https://tools.ietf.org/html/rfc4960#section-5.2.1
                                     // Upon receipt of an INIT in the COOKIE-WAIT state, an endpoint MUST
                                     // respond with an INIT ACK using the same parameters it sent in its
                                     // original INIT chunk (including its Initiate Tag, unchanged).  When
                                     // responding, the endpoint MUST send the INIT ACK back to the same
                                     // address that the original INIT (sent by this endpoint) was sent.

                                     if state != closed && state != CookieWait && state != CookieEchoed {
                                         // 5.2.2.  Unexpected INIT in States Other than CLOSED, COOKIE-ECHOED,
                                         //        COOKIE-WAIT, and SHUTDOWN-ACK-SENT
                                         return nil, fmt.Errorf("%w: %s", errHandleInitState, getAssociationStateString(state))
                                     }

                                     // Should we be setting any of these permanently until we've ACKed further?
                                     a.my_max_num_inbound_streams = min16(i.numInboundStreams, a.my_max_num_inbound_streams)
                                     a.my_max_num_outbound_streams = min16(i.numOutboundStreams, a.my_max_num_outbound_streams)
                                     a.peer_verification_tag = i.initiateTag
                                     a.source_port = p.destination_port
                                     a.destination_port = p.source_port

                                     // 13.2 This is the last TSN received in sequence.  This value
                                     // is set initially by taking the peer's initial TSN,
                                     // received in the INIT or INIT ACK chunk, and
                                     // subtracting one from it.
                                     a.peer_last_tsn = i.initialTSN - 1

                                     for _, param := range i.params {
                                         switch v := param.(type) { // nolint:gocritic
                                         case *paramSupportedExtensions:
                                             for _, t := range v.ChunkTypes {
                                                 if t == ctForwardTSN {
                                                     a.log.Debugf("[%s] use ForwardTSN (on init)\n", a.name)
                                                     a.use_forward_tsn = true
                                                 }
                                             }
                                         }
                                     }
                                     if !a.use_forward_tsn {
                                         a.log.Warnf("[%s] not using ForwardTSN (on init)\n", a.name)
                                     }

                                     outbound := &packet{}
                                     outbound.verificationTag = a.peer_verification_tag
                                     outbound.source_port = a.source_port
                                     outbound.destination_port = a.destination_port

                                     initAck := &chunkInitAck{}

                                     initAck.initialTSN = a.my_next_tsn
                                     initAck.numOutboundStreams = a.my_max_num_outbound_streams
                                     initAck.numInboundStreams = a.my_max_num_inbound_streams
                                     initAck.initiateTag = a.my_verification_tag
                                     initAck.advertisedReceiverWindowCredit = a.max_receive_buffer_size

                                     if a.my_cookie == nil {
                                         var err error
                                         if a.my_cookie, err = newRandomStateCookie(); err != nil {
                                             return nil, err
                                         }
                                     }

                                     initAck.params = []param{a.my_cookie}

                                     setSupportedExtensions(&initAck.chunkInitCommon)

                                     outbound.chunks = []chunk{initAck}

                                     return pack(outbound), nil
                                 }

                                 // The caller should hold the lock.
                                 fn handleInitAck(p *packet, i *chunkInitAck) error {
                                     state := a.get_state()
                                     a.log.Debugf("[%s] chunkInitAck received in state '%s'", a.name, getAssociationStateString(state))
                                     if state != CookieWait {
                                         // RFC 4960
                                         // 5.2.3.  Unexpected INIT ACK
                                         //   If an INIT ACK is received by an endpoint in any state other than the
                                         //   COOKIE-WAIT state, the endpoint should discard the INIT ACK chunk.
                                         //   An unexpected INIT ACK usually indicates the processing of an old or
                                         //   duplicated INIT chunk.
                                         return nil
                                     }

                                     a.my_max_num_inbound_streams = min16(i.numInboundStreams, a.my_max_num_inbound_streams)
                                     a.my_max_num_outbound_streams = min16(i.numOutboundStreams, a.my_max_num_outbound_streams)
                                     a.peer_verification_tag = i.initiateTag
                                     a.peer_last_tsn = i.initialTSN - 1
                                     if a.source_port != p.destination_port ||
                                         a.destination_port != p.source_port {
                                         a.log.Warnf("[%s] handleInitAck: port mismatch", a.name)
                                         return nil
                                     }

                                     a.rwnd = i.advertisedReceiverWindowCredit
                                     a.log.Debugf("[%s] initial rwnd=%d", a.name, a.rwnd)

                                     // RFC 4690 Sec 7.2.1
                                     //  o  The initial value of ssthresh MAY be arbitrarily high (for
                                     //     example, implementations MAY use the size of the receiver
                                     //     advertised window).
                                     a.ssthresh = a.rwnd
                                     a.log.Tracef("[%s] updated cwnd=%d ssthresh=%d inflight=%d (INI)",
                                         a.name, a.cwnd, a.ssthresh, a.inflight_queue.getNumBytes())

                                     a.t1init.stop()
                                     a.stored_init = nil

                                     var cookieParam *paramStateCookie
                                     for _, param := range i.params {
                                         switch v := param.(type) {
                                         case *paramStateCookie:
                                             cookieParam = v
                                         case *paramSupportedExtensions:
                                             for _, t := range v.ChunkTypes {
                                                 if t == ctForwardTSN {
                                                     a.log.Debugf("[%s] use ForwardTSN (on initAck)\n", a.name)
                                                     a.use_forward_tsn = true
                                                 }
                                             }
                                         }
                                     }
                                     if !a.use_forward_tsn {
                                         a.log.Warnf("[%s] not using ForwardTSN (on initAck)\n", a.name)
                                     }
                                     if cookieParam == nil {
                                         return errInitAckNoCookie
                                     }

                                     a.stored_cookie_echo = &chunkCookieEcho{}
                                     a.stored_cookie_echo.cookie = cookieParam.cookie

                                     err := a.send_cookie_echo()
                                     if err != nil {
                                         a.log.Errorf("[%s] failed to send init: %s", a.name, err.Error())
                                     }

                                     a.t1cookie.start(a.rto_mgr.getRTO())
                                     a.set_state(CookieEchoed)
                                     return nil
                                 }

                                 // The caller should hold the lock.
                                 fn handleHeartbeat(c *chunkHeartbeat) []*packet {
                                     a.log.Tracef("[%s] chunkHeartbeat", a.name)
                                     hbi, ok := c.params[0].(*paramHeartbeatInfo)
                                     if !ok {
                                         a.log.Warnf("[%s] failed to handle Heartbeat, no ParamHeartbeatInfo", a.name)
                                     }

                                     return pack(&packet{
                                         verificationTag: a.peer_verification_tag,
                                         source_port:      a.source_port,
                                         destination_port: a.destination_port,
                                         chunks: []chunk{&chunkHeartbeatAck{
                                             params: []param{
                                                 &paramHeartbeatInfo{
                                                     heartbeatInformation: hbi.heartbeatInformation,
                                                 },
                                             },
                                         }},
                                     })
                                 }

                                 // The caller should hold the lock.
                                 fn handleCookieEcho(c *chunkCookieEcho) []*packet {
                                     state := a.get_state()
                                     a.log.Debugf("[%s] COOKIE-ECHO received in state '%s'", a.name, getAssociationStateString(state))

                                     if a.my_cookie == nil {
                                         a.log.Debugf("[%s] COOKIE-ECHO received before initialization", a.name)
                                         return nil
                                     }
                                     switch state {
                                     default:
                                         return nil
                                     case Established:
                                         if !bytes.Equal(a.my_cookie.cookie, c.cookie) {
                                             return nil
                                         }
                                     case closed, CookieWait, CookieEchoed:
                                         if !bytes.Equal(a.my_cookie.cookie, c.cookie) {
                                             return nil
                                         }

                                         a.t1init.stop()
                                         a.stored_init = nil

                                         a.t1cookie.stop()
                                         a.stored_cookie_echo = nil

                                         a.set_state(Established)
                                         a.handshakeCompletedCh <- nil
                                     }

                                     p := &packet{
                                         verificationTag: a.peer_verification_tag,
                                         source_port:      a.source_port,
                                         destination_port: a.destination_port,
                                         chunks:          []chunk{&chunkCookieAck{}},
                                     }
                                     return pack(p)
                                 }

                                 // The caller should hold the lock.
                                 fn handleCookieAck() {
                                     state := a.get_state()
                                     a.log.Debugf("[%s] COOKIE-ACK received in state '%s'", a.name, getAssociationStateString(state))
                                     if state != CookieEchoed {
                                         // RFC 4960
                                         // 5.2.5.  Handle Duplicate COOKIE-ACK.
                                         //   At any state other than COOKIE-ECHOED, an endpoint should silently
                                         //   discard a received COOKIE ACK chunk.
                                         return
                                     }

                                     a.t1cookie.stop()
                                     a.stored_cookie_echo = nil

                                     a.set_state(Established)
                                     a.handshakeCompletedCh <- nil
                                 }
    */
    // The caller should hold the lock.
    fn handle_data(&mut self, d: ChunkPayloadData) -> Option<Vec<Packet>> {
        log::trace!(
            "[{}] DATA: tsn={} immediateSack={} len={}",
            self.name,
            d.tsn,
            d.immediate_sack,
            d.user_data.len()
        );
        self.stats.inc_datas();

        let can_push = self.payload_queue.can_push(&d, self.peer_last_tsn);
        let mut stream_handle_data = false;
        if can_push {
            if let Some(_s) = self.get_or_create_stream(d.stream_identifier) {
                if self.get_my_receiver_window_credit() > 0 {
                    // Pass the new chunk to stream level as soon as it arrives
                    self.payload_queue.push(d.clone(), self.peer_last_tsn);
                    stream_handle_data = true; //s.handle_data(d.clone());
                } else {
                    // Receive buffer is full
                    if let Some(last_tsn) = self.payload_queue.get_last_tsn_received() {
                        if sna32lt(d.tsn, *last_tsn) {
                            log::debug!("[{}] receive buffer full, but accepted as this is a missing chunk with tsn={} ssn={}", self.name, d.tsn, d.stream_sequence_number);
                            self.payload_queue.push(d.clone(), self.peer_last_tsn);
                            stream_handle_data = true; //s.handle_data(d.clone());
                        }
                    } else {
                        log::debug!(
                            "[{}] receive buffer full. dropping DATA with tsn={} ssn={}",
                            self.name,
                            d.tsn,
                            d.stream_sequence_number
                        );
                    }
                }
            } else {
                // silently discard the data. (sender will retry on T3-rtx timeout)
                // see pion/sctp#30
                log::debug!("discard {}", d.stream_sequence_number);
                return None;
            }
        }

        let immediate_sack = d.immediate_sack;

        if stream_handle_data {
            if let Some(s) = self.streams.get_mut(&d.stream_identifier) {
                s.handle_data(d);
            }
        }

        self.handle_peer_last_tsn_and_acknowledgement(immediate_sack)
    }

    /// A common routine for handle_data and handle_forward_tsn routines
    /// The caller should hold the lock.
    fn handle_peer_last_tsn_and_acknowledgement(
        &mut self,
        sack_immediately: bool,
    ) -> Option<Vec<Packet>> {
        let mut reply = vec![];

        // Try to advance peer_last_tsn

        // From RFC 3758 Sec 3.6:
        //   .. and then MUST further advance its cumulative TSN point locally
        //   if possible
        // Meaning, if peer_last_tsn+1 points to a chunk that is received,
        // advance peer_last_tsn until peer_last_tsn+1 points to unreceived chunk.
        while self.payload_queue.pop(self.peer_last_tsn + 1).is_none() {
            self.peer_last_tsn += 1;

            //TODO: optimize it without clone?
            let rst_reqs: Vec<ParamOutgoingResetRequest> =
                self.reconfig_requests.values().cloned().collect();
            for rst_req in rst_reqs {
                let resp = self.reset_streams_if_any(&rst_req);
                log::debug!("[{}] RESET RESPONSE: {}", self.name, resp);
                reply.push(resp);
            }
        }

        let has_packet_loss = self.payload_queue.len() > 0;
        if has_packet_loss {
            log::trace!(
                "[{}] packetloss: {}",
                self.name,
                self.payload_queue
                    .get_gap_ack_blocks_string(self.peer_last_tsn)
            );
        }

        if (self.ack_state != AckState::Immediate
            && !sack_immediately
            && !has_packet_loss
            && self.ack_mode == AckMode::Normal)
            || self.ack_mode == AckMode::AlwaysDelay
        {
            if self.ack_state == AckState::Idle {
                self.delayed_ack_triggered = true;
            } else {
                self.immediate_ack_triggered = true;
            }
        } else {
            self.immediate_ack_triggered = true;
        }

        Some(reply)
    }

    /// The caller should hold the lock.
    fn get_my_receiver_window_credit(&self) -> u32 {
        let mut bytes_queued = 0;
        for s in self.streams.values() {
            bytes_queued += s.get_num_bytes_in_reassembly_queue() as u32;
        }

        if bytes_queued >= self.max_receive_buffer_size {
            0
        } else {
            self.max_receive_buffer_size - bytes_queued
        }
    }
    /*
                                             // OpenStream opens a stream
                                             fn OpenStream(streamIdentifier uint16, defaultPayloadType PayloadProtocolIdentifier) (*Stream, error) {
                                                 a.lock.Lock()
                                                 defer a.lock.Unlock()

                                                 if _, ok := a.streams[streamIdentifier]; ok {
                                                     return nil, fmt.Errorf("%w: %d", errStreamAlreadyExist, streamIdentifier)
                                                 }

                                                 s := a.create_stream(streamIdentifier, false)
                                                 s.setDefaultPayloadType(defaultPayloadType)

                                                 return s, nil
                                             }

                                             // AcceptStream accepts a stream
                                             fn AcceptStream() (*Stream, error) {
                                                 s, ok := <-a.acceptCh
                                                 if !ok {
                                                     return nil, io.EOF // no more incoming streams
                                                 }
                                                 return s, nil
                                             }
    */
    /// create_stream creates a stream. The caller should hold the lock and check no stream exists for this id.
    fn create_stream(&mut self, stream_identifier: u16, _accept: bool) -> Option<&Stream> {
        /* TODO: let s = Stream{
            //TODO: association:      a,
            stream_identifier: stream_identifier,
            reassemblyQueue:  newReassemblyQueue(stream_identifier),
            log:              a.log,
            name:             fmt.Sprintf("%d:%s", stream_identifier, a.name),
        }

        //TODO: s.readNotifier = sync.NewCond(&s.lock)

        if accept {
            select {
            case a.acceptCh <- s:
                a.streams[stream_identifier] = s
                a.log.Debugf("[%s] accepted a new stream (stream_identifier: %d)",
                    a.name, stream_identifier)
            default:
                a.log.Debugf("[%s] dropped a new stream (acceptCh size: %d)",
                    a.name, len(a.acceptCh))
                return nil
            }
        } else {
            a.streams[stream_identifier] = s
        }

        return s
         */
        self.streams.get(&stream_identifier)
    }

    /// get_or_create_stream gets or creates a stream. The caller should hold the lock.
    fn get_or_create_stream(&mut self, stream_identifier: u16) -> Option<&Stream> {
        if self.streams.contains_key(&stream_identifier) {
            self.streams.get(&stream_identifier)
        } else {
            self.create_stream(stream_identifier, true)
        }
    }
    /*
                                         // The caller should hold the lock.
                                         fn processSelectiveAck(d *chunkSelectiveAck) (map[uint16]int, uint32, error) { // nolint:gocognit
                                             bytesAckedPerStream := map[uint16]int{}

                                             // New ack point, so pop all ACKed packets from inflight_queue
                                             // We add 1 because the "currentAckPoint" has already been popped from the inflight queue
                                             // For the first SACK we take care of this by setting the ackpoint to cumAck - 1
                                             for i := a.cumulative_tsnack_point + 1; sna32LTE(i, d.cumulativeTSNAck); i++ {
                                                 c, ok := a.inflight_queue.pop(i)
                                                 if !ok {
                                                     return nil, 0, fmt.Errorf("%w: %v", errInflightQueueTSNPop, i)
                                                 }

                                                 if !c.acked {
                                                     // RFC 4096 sec 6.3.2.  Retransmission Timer Rules
                                                     //   R3)  Whenever a SACK is received that acknowledges the DATA chunk
                                                     //        with the earliest outstanding TSN for that address, restart the
                                                     //        T3-rtx timer for that address with its current RTO (if there is
                                                     //        still outstanding data on that address).
                                                     if i == a.cumulative_tsnack_point+1 {
                                                         // T3 timer needs to be reset. Stop it for now.
                                                         a.t3rtx.stop()
                                                     }

                                                     nBytesAcked := len(c.userData)

                                                     // Sum the number of bytes acknowledged per stream
                                                     if amount, ok := bytesAckedPerStream[c.streamIdentifier]; ok {
                                                         bytesAckedPerStream[c.streamIdentifier] = amount + nBytesAcked
                                                     } else {
                                                         bytesAckedPerStream[c.streamIdentifier] = nBytesAcked
                                                     }

                                                     // RFC 4960 sec 6.3.1.  RTO Calculation
                                                     //   C4)  When data is in flight and when allowed by rule C5 below, a new
                                                     //        RTT measurement MUST be made each round trip.  Furthermore, new
                                                     //        RTT measurements SHOULD be made no more than once per round trip
                                                     //        for a given destination transport address.
                                                     //   C5)  Karn's algorithm: RTT measurements MUST NOT be made using
                                                     //        packets that were retransmitted (and thus for which it is
                                                     //        ambiguous whether the reply was for the first instance of the
                                                     //        chunk or for a later instance)
                                                     if c.nSent == 1 && sna32GTE(c.tsn, a.min_tsn2measure_rtt) {
                                                         a.min_tsn2measure_rtt = a.my_next_tsn
                                                         rtt := time.Since(c.since).Seconds() * 1000.0
                                                         srtt := a.rto_mgr.setNewRTT(rtt)
                                                         a.log.Tracef("[%s] SACK: measured-rtt=%f srtt=%f new-rto=%f",
                                                             a.name, rtt, srtt, a.rto_mgr.getRTO())
                                                     }
                                                 }

                                                 if a.in_fast_recovery && c.tsn == a.fast_recover_exit_point {
                                                     a.log.Debugf("[%s] exit fast-recovery", a.name)
                                                     a.in_fast_recovery = false
                                                 }
                                             }

                                             htna := d.cumulativeTSNAck

                                             // Mark selectively acknowledged chunks as "acked"
                                             for _, g := range d.gapAckBlocks {
                                                 for i := g.start; i <= g.end; i++ {
                                                     tsn := d.cumulativeTSNAck + uint32(i)
                                                     c, ok := a.inflight_queue.get(tsn)
                                                     if !ok {
                                                         return nil, 0, fmt.Errorf("%w: %v", errTSNRequestNotExist, tsn)
                                                     }

                                                     if !c.acked {
                                                         nBytesAcked := a.inflight_queue.markAsAcked(tsn)

                                                         // Sum the number of bytes acknowledged per stream
                                                         if amount, ok := bytesAckedPerStream[c.streamIdentifier]; ok {
                                                             bytesAckedPerStream[c.streamIdentifier] = amount + nBytesAcked
                                                         } else {
                                                             bytesAckedPerStream[c.streamIdentifier] = nBytesAcked
                                                         }

                                                         a.log.Tracef("[%s] tsn=%d has been sacked", a.name, c.tsn)

                                                         if c.nSent == 1 {
                                                             a.min_tsn2measure_rtt = a.my_next_tsn
                                                             rtt := time.Since(c.since).Seconds() * 1000.0
                                                             srtt := a.rto_mgr.setNewRTT(rtt)
                                                             a.log.Tracef("[%s] SACK: measured-rtt=%f srtt=%f new-rto=%f",
                                                                 a.name, rtt, srtt, a.rto_mgr.getRTO())
                                                         }

                                                         if sna32LT(htna, tsn) {
                                                             htna = tsn
                                                         }
                                                     }
                                                 }
                                             }

                                             return bytesAckedPerStream, htna, nil
                                         }

                                         // The caller should hold the lock.
                                         fn onCumulativeTSNAckPointAdvanced(totalBytesAcked int) {
                                             // RFC 4096, sec 6.3.2.  Retransmission Timer Rules
                                             //   R2)  Whenever all outstanding data sent to an address have been
                                             //        acknowledged, turn off the T3-rtx timer of that address.
                                             if a.inflight_queue.size() == 0 {
                                                 a.log.Tracef("[%s] SACK: no more packet in-flight (pending=%d)", a.name, a.pending_queue.size())
                                                 a.t3rtx.stop()
                                             } else {
                                                 a.log.Tracef("[%s] T3-rtx timer start (pt2)", a.name)
                                                 a.t3rtx.start(a.rto_mgr.getRTO())
                                             }

                                             // Update congestion control parameters
                                             if a.cwnd <= a.ssthresh {
                                                 // RFC 4096, sec 7.2.1.  Slow-Start
                                                 //   o  When cwnd is less than or equal to ssthresh, an SCTP endpoint MUST
                                                 //		use the slow-start algorithm to increase cwnd only if the current
                                                 //      congestion window is being fully utilized, an incoming SACK
                                                 //      advances the Cumulative TSN Ack Point, and the data sender is not
                                                 //      in Fast Recovery.  Only when these three conditions are met can
                                                 //      the cwnd be increased; otherwise, the cwnd MUST not be increased.
                                                 //		If these conditions are met, then cwnd MUST be increased by, at
                                                 //      most, the lesser of 1) the total size of the previously
                                                 //      outstanding DATA chunk(s) acknowledged, and 2) the destination's
                                                 //      path MTU.
                                                 if !a.in_fast_recovery &&
                                                     a.pending_queue.size() > 0 {
                                                     a.cwnd += min32(uint32(totalBytesAcked), a.cwnd) // TCP way
                                                     // a.cwnd += min32(uint32(totalBytesAcked), a.mtu) // SCTP way (slow)
                                                     a.log.Tracef("[%s] updated cwnd=%d ssthresh=%d acked=%d (SS)",
                                                         a.name, a.cwnd, a.ssthresh, totalBytesAcked)
                                                 } else {
                                                     a.log.Tracef("[%s] cwnd did not grow: cwnd=%d ssthresh=%d acked=%d FR=%v pending=%d",
                                                         a.name, a.cwnd, a.ssthresh, totalBytesAcked, a.in_fast_recovery, a.pending_queue.size())
                                                 }
                                             } else {
                                                 // RFC 4096, sec 7.2.2.  Congestion Avoidance
                                                 //   o  Whenever cwnd is greater than ssthresh, upon each SACK arrival
                                                 //      that advances the Cumulative TSN Ack Point, increase
                                                 //      partial_bytes_acked by the total number of bytes of all new chunks
                                                 //      acknowledged in that SACK including chunks acknowledged by the new
                                                 //      Cumulative TSN Ack and by Gap Ack Blocks.
                                                 a.partial_bytes_acked += uint32(totalBytesAcked)

                                                 //   o  When partial_bytes_acked is equal to or greater than cwnd and
                                                 //      before the arrival of the SACK the sender had cwnd or more bytes
                                                 //      of data outstanding (i.e., before arrival of the SACK, flight size
                                                 //      was greater than or equal to cwnd), increase cwnd by MTU, and
                                                 //      reset partial_bytes_acked to (partial_bytes_acked - cwnd).
                                                 if a.partial_bytes_acked >= a.cwnd && a.pending_queue.size() > 0 {
                                                     a.partial_bytes_acked -= a.cwnd
                                                     a.cwnd += a.mtu
                                                     a.log.Tracef("[%s] updated cwnd=%d ssthresh=%d acked=%d (CA)",
                                                         a.name, a.cwnd, a.ssthresh, totalBytesAcked)
                                                 }
                                             }
                                         }

                                         // The caller should hold the lock.
                                         fn processFastRetransmission(cumTSNAckPoint, htna uint32, cumTSNAckPointAdvanced bool) error {
                                             // HTNA algorithm - RFC 4960 Sec 7.2.4
                                             // Increment missIndicator of each chunks that the SACK reported missing
                                             // when either of the following is met:
                                             // a)  Not in fast-recovery
                                             //     miss indications are incremented only for missing TSNs prior to the
                                             //     highest TSN newly acknowledged in the SACK.
                                             // b)  In fast-recovery AND the Cumulative TSN Ack Point advanced
                                             //     the miss indications are incremented for all TSNs reported missing
                                             //     in the SACK.
                                             if !a.in_fast_recovery || (a.in_fast_recovery && cumTSNAckPointAdvanced) {
                                                 var maxTSN uint32
                                                 if !a.in_fast_recovery {
                                                     // a) increment only for missing TSNs prior to the HTNA
                                                     maxTSN = htna
                                                 } else {
                                                     // b) increment for all TSNs reported missing
                                                     maxTSN = cumTSNAckPoint + uint32(a.inflight_queue.size()) + 1
                                                 }

                                                 for tsn := cumTSNAckPoint + 1; sna32LT(tsn, maxTSN); tsn++ {
                                                     c, ok := a.inflight_queue.get(tsn)
                                                     if !ok {
                                                         return fmt.Errorf("%w: %v", errTSNRequestNotExist, tsn)
                                                     }
                                                     if !c.acked && !c.abandoned() && c.missIndicator < 3 {
                                                         c.missIndicator++
                                                         if c.missIndicator == 3 {
                                                             if !a.in_fast_recovery {
                                                                 // 2)  If not in Fast Recovery, adjust the ssthresh and cwnd of the
                                                                 //     destination address(es) to which the missing DATA chunks were
                                                                 //     last sent, according to the formula described in Section 7.2.3.
                                                                 a.in_fast_recovery = true
                                                                 a.fast_recover_exit_point = htna
                                                                 a.ssthresh = max32(a.cwnd/2, 4*a.mtu)
                                                                 a.cwnd = a.ssthresh
                                                                 a.partial_bytes_acked = 0
                                                                 a.will_retransmit_fast = true

                                                                 a.log.Tracef("[%s] updated cwnd=%d ssthresh=%d inflight=%d (FR)",
                                                                     a.name, a.cwnd, a.ssthresh, a.inflight_queue.getNumBytes())
                                                             }
                                                         }
                                                     }
                                                 }
                                             }

                                             if a.in_fast_recovery && cumTSNAckPointAdvanced {
                                                 a.will_retransmit_fast = true
                                             }

                                             return nil
                                         }

                                         // The caller should hold the lock.
                                         fn handleSack(d *chunkSelectiveAck) error {
                                             a.log.Tracef("[%s] SACK: cumTSN=%d a_rwnd=%d", a.name, d.cumulativeTSNAck, d.advertisedReceiverWindowCredit)
                                             state := a.get_state()
                                             if state != Established && state != ShutdownPending && state != ShutdownReceived {
                                                 return nil
                                             }

                                             a.stats.inc_sacks()

                                             if sna32GT(a.cumulative_tsnack_point, d.cumulativeTSNAck) {
                                                 // RFC 4960 sec 6.2.1.  Processing a Received SACK
                                                 // D)
                                                 //   i) If Cumulative TSN Ack is less than the Cumulative TSN Ack
                                                 //      Point, then drop the SACK.  Since Cumulative TSN Ack is
                                                 //      monotonically increasing, a SACK whose Cumulative TSN Ack is
                                                 //      less than the Cumulative TSN Ack Point indicates an out-of-
                                                 //      order SACK.

                                                 a.log.Debugf("[%s] SACK Cumulative ACK %v is older than ACK point %v",
                                                     a.name,
                                                     d.cumulativeTSNAck,
                                                     a.cumulative_tsnack_point)

                                                 return nil
                                             }

                                             // Process selective ack
                                             bytesAckedPerStream, htna, err := a.processSelectiveAck(d)
                                             if err != nil {
                                                 return err
                                             }

                                             var totalBytesAcked int
                                             for _, nBytesAcked := range bytesAckedPerStream {
                                                 totalBytesAcked += nBytesAcked
                                             }

                                             cumTSNAckPointAdvanced := false
                                             if sna32LT(a.cumulative_tsnack_point, d.cumulativeTSNAck) {
                                                 a.log.Tracef("[%s] SACK: cumTSN advanced: %d -> %d",
                                                     a.name,
                                                     a.cumulative_tsnack_point,
                                                     d.cumulativeTSNAck)

                                                 a.cumulative_tsnack_point = d.cumulativeTSNAck
                                                 cumTSNAckPointAdvanced = true
                                                 a.onCumulativeTSNAckPointAdvanced(totalBytesAcked)
                                             }

                                             for si, nBytesAcked := range bytesAckedPerStream {
                                                 if s, ok := a.streams[si]; ok {
                                                     a.lock.Unlock()
                                                     s.onBufferReleased(nBytesAcked)
                                                     a.lock.Lock()
                                                 }
                                             }

                                             // New rwnd value
                                             // RFC 4960 sec 6.2.1.  Processing a Received SACK
                                             // D)
                                             //   ii) Set rwnd equal to the newly received a_rwnd minus the number
                                             //       of bytes still outstanding after processing the Cumulative
                                             //       TSN Ack and the Gap Ack Blocks.

                                             // bytes acked were already subtracted by markAsAcked() method
                                             bytesOutstanding := uint32(a.inflight_queue.getNumBytes())
                                             if bytesOutstanding >= d.advertisedReceiverWindowCredit {
                                                 a.rwnd = 0
                                             } else {
                                                 a.rwnd = d.advertisedReceiverWindowCredit - bytesOutstanding
                                             }

                                             err = a.processFastRetransmission(d.cumulativeTSNAck, htna, cumTSNAckPointAdvanced)
                                             if err != nil {
                                                 return err
                                             }

                                             if a.use_forward_tsn {
                                                 // RFC 3758 Sec 3.5 C1
                                                 if sna32LT(a.advanced_peer_tsnack_point, a.cumulative_tsnack_point) {
                                                     a.advanced_peer_tsnack_point = a.cumulative_tsnack_point
                                                 }

                                                 // RFC 3758 Sec 3.5 C2
                                                 for i := a.advanced_peer_tsnack_point + 1; ; i++ {
                                                     c, ok := a.inflight_queue.get(i)
                                                     if !ok {
                                                         break
                                                     }
                                                     if !c.abandoned() {
                                                         break
                                                     }
                                                     a.advanced_peer_tsnack_point = i
                                                 }

                                                 // RFC 3758 Sec 3.5 C3
                                                 if sna32GT(a.advanced_peer_tsnack_point, a.cumulative_tsnack_point) {
                                                     a.will_send_forward_tsn = true
                                                 }
                                                 a.awake_write_loop()
                                             }

                                             a.postprocessSack(state, cumTSNAckPointAdvanced)

                                             return nil
                                         }

                                         // The caller must hold the lock. This method was only added because the
                                         // linter was complaining about the "cognitive complexity" of handleSack.
                                         fn postprocessSack(state uint32, shouldAwakeWriteLoop bool) {
                                             switch {
                                             case a.inflight_queue.size() > 0:
                                                 // Start timer. (noop if already started)
                                                 a.log.Tracef("[%s] T3-rtx timer start (pt3)", a.name)
                                                 a.t3rtx.start(a.rto_mgr.getRTO())
                                             case state == ShutdownPending:
                                                 // No more outstanding, send shutdown.
                                                 shouldAwakeWriteLoop = true
                                                 a.will_send_shutdown = true
                                                 a.set_state(ShutdownSent)
                                             case state == ShutdownReceived:
                                                 // No more outstanding, send shutdown ack.
                                                 shouldAwakeWriteLoop = true
                                                 a.will_send_shutdown_ack = true
                                                 a.set_state(ShutdownAckSent)
                                             }

                                             if shouldAwakeWriteLoop {
                                                 a.awake_write_loop()
                                             }
                                         }

                                         // The caller should hold the lock.
                                         fn handleShutdown(_ *chunkShutdown) {
                                             state := a.get_state()

                                             switch state {
                                             case Established:
                                                 if a.inflight_queue.size() > 0 {
                                                     a.set_state(ShutdownReceived)
                                                 } else {
                                                     // No more outstanding, send shutdown ack.
                                                     a.will_send_shutdown_ack = true
                                                     a.set_state(ShutdownAckSent)

                                                     a.awake_write_loop()
                                                 }

                                                 // a.cumulative_tsnack_point = c.cumulativeTSNAck
                                             case ShutdownSent:
                                                 a.will_send_shutdown_ack = true
                                                 a.set_state(ShutdownAckSent)

                                                 a.awake_write_loop()
                                             }
                                         }

                                         // The caller should hold the lock.
                                         fn handleShutdownAck(_ *chunkShutdownAck) {
                                             state := a.get_state()
                                             if state == ShutdownSent || state == ShutdownAckSent {
                                                 a.t2shutdown.stop()
                                                 a.will_send_shutdown_complete = true

                                                 a.awake_write_loop()
                                             }
                                         }

                                         fn handleShutdownComplete(_ *chunkShutdownComplete) error {
                                             state := a.get_state()
                                             if state == ShutdownAckSent {
                                                 a.t2shutdown.stop()

                                                 return a.close()
                                             }

                                             return nil
                                         }
    */
    /// create_forward_tsn generates ForwardTSN chunk.
    /// This method will be be called if use_forward_tsn is set to false.
    /// The caller should hold the lock.
    fn create_forward_tsn(&self) -> ChunkForwardTsn {
        // RFC 3758 Sec 3.5 C4
        let mut stream_map: HashMap<u16, u16> = HashMap::new(); // to report only once per SI
        let mut i = self.cumulative_tsnack_point + 1;
        while sna32lte(i, self.advanced_peer_tsnack_point) {
            if let Some(c) = self.inflight_queue.get(i) {
                if let Some(ssn) = stream_map.get(&c.stream_identifier) {
                    if sna16lt(*ssn, c.stream_sequence_number) {
                        // to report only once with greatest SSN
                        stream_map.insert(c.stream_identifier, c.stream_sequence_number);
                    }
                } else {
                    stream_map.insert(c.stream_identifier, c.stream_sequence_number);
                }
            } else {
                break;
            }

            i += 1;
        }

        let mut fwd_tsn = ChunkForwardTsn {
            new_cumulative_tsn: self.advanced_peer_tsnack_point,
            streams: vec![],
        };

        let mut stream_str = String::new();
        for (si, ssn) in &stream_map {
            stream_str += format!("(si={} ssn={})", si, ssn).as_str();
            fwd_tsn.streams.push(ChunkForwardTsnStream {
                identifier: *si,
                sequence: *ssn,
            });
        }
        log::trace!(
            "[{}] building fwd_tsn: newCumulativeTSN={} cumTSN={} - {}",
            self.name,
            fwd_tsn.new_cumulative_tsn,
            self.cumulative_tsnack_point,
            stream_str
        );

        fwd_tsn
    }

    /// create_packet wraps chunks in a packet.
    /// The caller should hold the read lock.
    fn create_packet(&self, chunks: Vec<Box<dyn Chunk>>) -> Packet {
        Packet {
            verification_tag: self.peer_verification_tag,
            source_port: self.source_port,
            destination_port: self.destination_port,
            chunks,
        }
    }

    /// The caller should hold the lock.
    async fn handle_reconfig(&mut self, c: ChunkReconfig) -> Result<Vec<Packet>, Error> {
        log::trace!("[{}] handle_reconfig", self.name);

        let mut pp = vec![];

        if let Some(param_a) = &c.param_a {
            if let Some(p) = self.handle_reconfig_param(param_a).await? {
                pp.push(p);
            }
        }

        if let Some(param_b) = &c.param_b {
            if let Some(p) = self.handle_reconfig_param(param_b).await? {
                pp.push(p);
            }
        }

        Ok(pp)
    }

    /// The caller should hold the lock.
    fn handle_forward_tsn(&mut self, c: ChunkForwardTsn) -> Option<Vec<Packet>> {
        log::trace!("[{}] FwdTSN: {}", self.name, c.to_string());

        if !self.use_forward_tsn {
            log::warn!("[{}] received FwdTSN but not enabled", self.name);
            // Return an error chunk
            let cerr = ChunkError {
                error_causes: vec![ErrorCauseUnrecognizedChunkType::default()],
            };

            let outbound = Packet {
                verification_tag: self.peer_verification_tag,
                source_port: self.source_port,
                destination_port: self.destination_port,
                chunks: vec![Box::new(cerr)],
            };
            return Some(vec![outbound]);
        }

        // From RFC 3758 Sec 3.6:
        //   Note, if the "New Cumulative TSN" value carried in the arrived
        //   FORWARD TSN chunk is found to be behind or at the current cumulative
        //   TSN point, the data receiver MUST treat this FORWARD TSN as out-of-
        //   date and MUST NOT update its Cumulative TSN.  The receiver SHOULD
        //   send a SACK to its peer (the sender of the FORWARD TSN) since such a
        //   duplicate may indicate the previous SACK was lost in the network.

        log::trace!(
            "[{}] should send ack? newCumTSN={} peer_last_tsn={}",
            self.name,
            c.new_cumulative_tsn,
            self.peer_last_tsn
        );
        if sna32lte(c.new_cumulative_tsn, self.peer_last_tsn) {
            log::trace!("[{}] sending ack on Forward TSN", self.name);
            self.ack_state = AckState::Immediate;
            self.ack_timer.stop();
            self.awake_write_loop();
            return None;
        }

        // From RFC 3758 Sec 3.6:
        //   the receiver MUST perform the same TSN handling, including duplicate
        //   detection, gap detection, SACK generation, cumulative TSN
        //   advancement, etc. as defined in RFC 2960 [2]---with the following
        //   exceptions and additions.

        //   When a FORWARD TSN chunk arrives, the data receiver MUST first update
        //   its cumulative TSN point to the value carried in the FORWARD TSN
        //   chunk,

        // Advance peer_last_tsn
        while sna32lt(self.peer_last_tsn, c.new_cumulative_tsn) {
            self.payload_queue.pop(self.peer_last_tsn + 1); // may not exist
            self.peer_last_tsn += 1;
        }

        // Report new peer_last_tsn value and abandoned largest SSN value to
        // corresponding streams so that the abandoned chunks can be removed
        // from the reassemblyQueue.
        for forwarded in &c.streams {
            if let Some(s) = self.streams.get_mut(&forwarded.identifier) {
                s.handle_forward_tsn_for_ordered(forwarded.sequence);
            }
        }

        // TSN may be forewared for unordered chunks. ForwardTSN chunk does not
        // report which stream identifier it skipped for unordered chunks.
        // Therefore, we need to broadcast this event to all existing streams for
        // unordered chunks.
        // See https://github.com/pion/sctp/issues/106
        for s in self.streams.values_mut() {
            s.handle_forward_tsn_for_unordered(c.new_cumulative_tsn);
        }

        self.handle_peer_last_tsn_and_acknowledgement(false)
    }

    fn send_reset_request(&mut self, stream_identifier: u16) -> Result<(), Error> {
        let state = self.get_state();
        if state != AssociationState::Established {
            return Err(Error::ErrResetPacketInStateNotExist);
        }

        // Create DATA chunk which only contains valid stream identifier with
        // nil userData and use it as a EOS from the stream.
        let c = ChunkPayloadData {
            stream_identifier,
            beginning_fragment: true,
            ending_fragment: true,
            user_data: Bytes::new(),
            ..Default::default()
        };

        self.pending_queue.push(c);
        self.awake_write_loop();

        Ok(())
    }

    /// The caller should hold the lock.
    #[allow(clippy::borrowed_box)]
    async fn handle_reconfig_param(
        &mut self,
        raw: &Box<dyn Param>,
    ) -> Result<Option<Packet>, Error> {
        if let Some(p) = raw.as_any().downcast_ref::<ParamOutgoingResetRequest>() {
            self.reconfig_requests
                .insert(p.reconfig_request_sequence_number, p.clone());
            Ok(Some(self.reset_streams_if_any(p)))
        } else if let Some(p) = raw.as_any().downcast_ref::<ParamReconfigResponse>() {
            self.reconfigs.remove(&p.reconfig_response_sequence_number);
            if self.reconfigs.is_empty() {
                self.treconfig.stop().await;
            }
            Ok(None)
        } else {
            Err(Error::ErrParamterType)
        }
    }

    /// The caller should hold the lock.
    fn reset_streams_if_any(&mut self, p: &ParamOutgoingResetRequest) -> Packet {
        let mut result = ReconfigResult::SuccessPerformed;
        if sna32lte(p.sender_last_tsn, self.peer_last_tsn) {
            log::debug!(
                "[{}] resetStream(): senderLastTSN={} <= peer_last_tsn={}",
                self.name,
                p.sender_last_tsn,
                self.peer_last_tsn
            );
            for id in &p.stream_identifiers {
                if let Some(s) = self.streams.get(id) {
                    let stream_identifier = s.stream_identifier;
                    self.unregister_stream(stream_identifier, Error::ErrEof);
                }
            }
            self.reconfig_requests
                .remove(&p.reconfig_request_sequence_number);
        } else {
            log::debug!(
                "[{}] resetStream(): senderLastTSN={} > peer_last_tsn={}",
                self.name,
                p.sender_last_tsn,
                self.peer_last_tsn
            );
            result = ReconfigResult::InProgress;
        }

        self.create_packet(vec![Box::new(ChunkReconfig {
            param_a: Some(Box::new(ParamReconfigResponse {
                reconfig_response_sequence_number: p.reconfig_request_sequence_number,
                result,
            })),
            param_b: None,
        })])
    }

    /// Move the chunk peeked with a.pending_queue.peek() to the inflight_queue.
    /// The caller should hold the lock.
    fn move_pending_data_chunk_to_inflight_queue(
        &mut self,
        beginning_fragment: bool,
        unordered: bool,
    ) -> Option<ChunkPayloadData> {
        if let Some(mut c) = self.pending_queue.pop(beginning_fragment, unordered) {
            // Mark all fragements are in-flight now
            if c.ending_fragment {
                c.set_all_inflight();
            }

            // Assign TSN
            c.tsn = self.generate_next_tsn();

            c.since = SystemTime::now(); // use to calculate RTT and also for maxPacketLifeTime
            c.nsent = 1; // being sent for the first time

            self.check_partial_reliability_status(&c);

            log::trace!(
                "[{}] sending ppi={} tsn={} ssn={} sent={} len={} ({},{})",
                self.name,
                c.payload_type,
                c.tsn,
                c.stream_sequence_number,
                c.nsent,
                c.user_data.len(),
                c.beginning_fragment,
                c.ending_fragment
            );

            self.inflight_queue.push_no_check(c.clone());

            Some(c)
        } else {
            log::error!("[{}] failed to pop from pending queue", self.name);
            None
        }
    }

    /// pop_pending_data_chunks_to_send pops chunks from the pending queues as many as
    /// the cwnd and rwnd allows to send.
    /// The caller should hold the lock.
    fn pop_pending_data_chunks_to_send(&mut self) -> (Vec<ChunkPayloadData>, Vec<u16>) {
        let mut chunks = vec![];
        let mut sis_to_reset = vec![]; // stream identifiers to reset
        let is_empty = self.pending_queue.len() == 0;
        if !is_empty {
            // RFC 4960 sec 6.1.  Transmission of DATA Chunks
            //   A) At any given time, the data sender MUST NOT transmit new data to
            //      any destination transport address if its peer's rwnd indicates
            //      that the peer has no buffer space (i.e., rwnd is 0; see Section
            //      6.2.1).  However, regardless of the value of rwnd (including if it
            //      is 0), the data sender can always have one DATA chunk in flight to
            //      the receiver if allowed by cwnd (see rule B, below).

            while let Some(c) = self.pending_queue.peek() {
                let (beginning_fragment, unordered, data_len, stream_identifier) = (
                    c.beginning_fragment,
                    c.unordered,
                    c.user_data.len(),
                    c.stream_identifier,
                );

                if data_len == 0 {
                    sis_to_reset.push(stream_identifier);
                    if self
                        .pending_queue
                        .pop(beginning_fragment, unordered)
                        .is_none()
                    {
                        log::error!("failed to pop from pending queue");
                    }
                    continue;
                }

                if self.inflight_queue.get_num_bytes() + data_len > self.cwnd as usize {
                    break; // would exceeds cwnd
                }

                if data_len > self.rwnd as usize {
                    break; // no more rwnd
                }

                self.rwnd -= data_len as u32;

                if let Some(chunk) =
                    self.move_pending_data_chunk_to_inflight_queue(beginning_fragment, unordered)
                {
                    chunks.push(chunk);
                }
            }

            // the data sender can always have one DATA chunk in flight to the receiver
            if chunks.is_empty() && self.inflight_queue.len() == 0 {
                // Send zero window probe
                if let Some(c) = self.pending_queue.peek() {
                    let (beginning_fragment, unordered) = (c.beginning_fragment, c.unordered);

                    if let Some(chunk) = self
                        .move_pending_data_chunk_to_inflight_queue(beginning_fragment, unordered)
                    {
                        chunks.push(chunk);
                    }
                }
            }
        }

        (chunks, sis_to_reset)
    }

    /// bundle_data_chunks_into_packets packs DATA chunks into packets. It tries to bundle
    /// DATA chunks into a packet so long as the resulting packet size does not exceed
    /// the path MTU.
    /// The caller should hold the lock.
    fn bundle_data_chunks_into_packets(&self, chunks: Vec<ChunkPayloadData>) -> Vec<Packet> {
        let mut packets = vec![];
        let mut chunks_to_send = vec![];
        let mut bytes_in_packet = COMMON_HEADER_SIZE;

        for c in chunks {
            // RFC 4960 sec 6.1.  Transmission of DATA Chunks
            //   Multiple DATA chunks committed for transmission MAY be bundled in a
            //   single packet.  Furthermore, DATA chunks being retransmitted MAY be
            //   bundled with new DATA chunks, as long as the resulting packet size
            //   does not exceed the path MTU.
            if bytes_in_packet + c.user_data.len() as u32 > self.mtu {
                packets.push(self.create_packet(chunks_to_send));
                chunks_to_send = vec![];
                bytes_in_packet = COMMON_HEADER_SIZE;
            }

            bytes_in_packet += DATA_CHUNK_HEADER_SIZE + c.user_data.len() as u32;
            chunks_to_send.push(Box::new(c));
        }

        if !chunks_to_send.is_empty() {
            packets.push(self.create_packet(chunks_to_send));
        }

        packets
    }

    /// send_payload_data sends the data chunks.
    fn send_payload_data(&mut self, chunks: Vec<ChunkPayloadData>) -> Result<(), Error> {
        let state = self.get_state();
        if state != AssociationState::Established {
            return Err(Error::ErrPayloadDataStateNotExist);
        }

        // Push the chunks into the pending queue first.
        for c in chunks {
            self.pending_queue.push(c);
        }

        self.awake_write_loop();
        Ok(())
    }

    /// The caller should hold the lock.
    fn check_partial_reliability_status(&self, c: &ChunkPayloadData) {
        if !self.use_forward_tsn {
            return;
        }

        // draft-ietf-rtcweb-data-protocol-09.txt section 6
        //	6.  Procedures
        //		All Data Channel Establishment Protocol messages MUST be sent using
        //		ordered delivery and reliable transmission.
        //
        if c.payload_type == PayloadProtocolIdentifier::Dcep {
            return;
        }

        // PR-SCTP
        if let Some(s) = self.streams.get(&c.stream_identifier) {
            if s.reliability_type == ReliabilityType::Rexmit {
                if c.nsent >= s.reliability_value {
                    c.set_abandoned(true);
                    log::trace!(
                        "[{}] marked as abandoned: tsn={} ppi={} (remix: {})",
                        self.name,
                        c.tsn,
                        c.payload_type,
                        c.nsent
                    );
                }
            } else if s.reliability_type == ReliabilityType::Timed {
                if let Ok(elapsed) = SystemTime::now().duration_since(c.since) {
                    if elapsed.as_millis() as u32 >= s.reliability_value {
                        c.set_abandoned(true);
                        log::trace!(
                            "[{}] marked as abandoned: tsn={} ppi={} (timed: {:?})",
                            self.name,
                            c.tsn,
                            c.payload_type,
                            elapsed
                        );
                    }
                }
            }
        } else {
            log::error!("[{}] stream {} not found)", self.name, c.stream_identifier);
        }
    }

    /// get_data_packets_to_retransmit is called when T3-rtx is timed out and retransmit outstanding data chunks
    /// that are not acked or abandoned yet.
    /// The caller should hold the lock.
    fn get_data_packets_to_retransmit(&mut self) -> Vec<Packet> {
        let awnd = std::cmp::min(self.cwnd, self.rwnd);
        let mut chunks = vec![];
        let mut bytes_to_send = 0;
        let mut done = false;
        let mut i = 0;
        while !done {
            let tsn = self.cumulative_tsnack_point + i + 1;
            if let Some(c) = self.inflight_queue.get_mut(tsn) {
                if !c.retransmit {
                    continue;
                }

                if i == 0 && self.rwnd < c.user_data.len() as u32 {
                    // Send it as a zero window probe
                    done = true;
                } else if bytes_to_send + c.user_data.len() > awnd as usize {
                    break;
                }

                // reset the retransmit flag not to retransmit again before the next
                // t3-rtx timer fires
                c.retransmit = false;
                bytes_to_send += c.user_data.len();

                c.nsent += 1;
            } else {
                break; // end of pending data
            }

            if let Some(c) = self.inflight_queue.get(tsn) {
                self.check_partial_reliability_status(c);

                log::trace!(
                    "[{}] retransmitting tsn={} ssn={} sent={}",
                    self.name,
                    c.tsn,
                    c.stream_sequence_number,
                    c.nsent
                );

                chunks.push(c.clone());
            }
            i += 1;
        }

        self.bundle_data_chunks_into_packets(chunks)
    }

    /// generate_next_tsn returns the my_next_tsn and increases it. The caller should hold the lock.
    /// The caller should hold the lock.
    fn generate_next_tsn(&mut self) -> u32 {
        let tsn = self.my_next_tsn;
        self.my_next_tsn += 1;
        tsn
    }

    /// generate_next_rsn returns the my_next_rsn and increases it. The caller should hold the lock.
    /// The caller should hold the lock.
    fn generate_next_rsn(&mut self) -> u32 {
        let rsn = self.my_next_rsn;
        self.my_next_rsn += 1;
        rsn
    }

    fn create_selective_ack_chunk(&mut self) -> ChunkSelectiveAck {
        ChunkSelectiveAck {
            cumulative_tsn_ack: self.peer_last_tsn,
            advertised_receiver_window_credit: self.get_my_receiver_window_credit(),
            gap_ack_blocks: self.payload_queue.get_gap_ack_blocks(self.peer_last_tsn),
            duplicate_tsn: self.payload_queue.pop_duplicates(),
        }
    }

    fn pack(p: Packet) -> Vec<Packet> {
        vec![p]
    }

    fn handle_chunk_start(&mut self) {
        self.delayed_ack_triggered = false;
        self.immediate_ack_triggered = false;
    }

    /*fn handleChunkEnd(&mut self) {
        if self.immediate_ack_triggered {
            self.ack_state = AckState::Immediate;
            self.ack_timer.stop();
            self.awake_write_loop();
        } else if self.delayed_ack_triggered {
            // Will send delayed ack in the next ack timeout
            self.ack_state = AckState::Delay;
            self.ack_timer.start(); //TODO:
        }
    }
                  fn handleChunk(p *packet, c chunk) error {
                      a.lock.Lock()
                      defer a.lock.Unlock()

                      var packets []*packet
                      var err error

                      if _, err = c.check(); err != nil {
                          a.log.Errorf("[ %s ] failed validating chunk: %s ", a.name, err)
                          return nil
                      }

                      switch c := c.(type) {
                      case *chunkInit:
                          packets, err = a.handleInit(p, c)

                      case *chunkInitAck:
                          err = a.handleInitAck(p, c)

                      case *chunkAbort:
                          var errStr string
                          for _, e := range c.errorCauses {
                              errStr += fmt.Sprintf("(%s)", e)
                          }
                          return fmt.Errorf("[%s] %w: %s", a.name, errChunk, errStr)

                      case *chunkError:
                          var errStr string
                          for _, e := range c.errorCauses {
                              errStr += fmt.Sprintf("(%s)", e)
                          }
                          a.log.Debugf("[%s] Error chunk, with following errors: %s", a.name, errStr)

                      case *chunkHeartbeat:
                          packets = a.handleHeartbeat(c)

                      case *chunkCookieEcho:
                          packets = a.handleCookieEcho(c)

                      case *chunkCookieAck:
                          a.handleCookieAck()

                      case *chunkPayloadData:
                          packets = a.handle_data(c)

                      case *chunkSelectiveAck:
                          err = a.handleSack(c)

                      case *chunkReconfig:
                          packets, err = a.handle_reconfig(c)

                      case *chunkForwardTSN:
                          packets = a.handle_forward_tsn(c)

                      case *chunkShutdown:
                          a.handleShutdown(c)
                      case *chunkShutdownAck:
                          a.handleShutdownAck(c)
                      case *chunkShutdownComplete:
                          err = a.handleShutdownComplete(c)

                      default:
                          err = errChunkTypeUnhandled
                      }

                      // Log and return, the only condition that is fatal is a ABORT chunk
                      if err != nil {
                          a.log.Errorf("Failed to handle chunk: %v", err)
                          return nil
                      }

                      if len(packets) > 0 {
                          a.control_queue.pushAll(packets)
                          a.awake_write_loop()
                      }

                      return nil
                  }

    */
    fn on_retransmission_timeout(&mut self, id: RtxTimerId, n_rtos: usize) {
        match id {
            RtxTimerId::T1Init => {
                if let Err(err) = self.send_init() {
                    log::debug!(
                        "[{}] failed to retransmit init (n_rtos={}): {:?}",
                        self.name,
                        n_rtos,
                        err
                    );
                }
            }

            RtxTimerId::T1Cookie => {
                if let Err(err) = self.send_cookie_echo() {
                    log::debug!(
                        "[{}] failed to retransmit cookie-echo (n_rtos={}): {:?}",
                        self.name,
                        n_rtos,
                        err
                    );
                }
            }

            RtxTimerId::T2Shutdown => {
                log::debug!(
                    "[{}] retransmission of shutdown timeout (n_rtos={})",
                    self.name,
                    n_rtos
                );
                let state = self.get_state();
                match state {
                    AssociationState::ShutdownSent => {
                        self.will_send_shutdown = true;
                        self.awake_write_loop();
                    }
                    AssociationState::ShutdownAckSent => {
                        self.will_send_shutdown_ack = true;
                        self.awake_write_loop();
                    }
                    _ => {}
                }
            }

            RtxTimerId::T3RTX => {
                self.stats.inc_t3timeouts();

                // RFC 4960 sec 6.3.3
                //  E1)  For the destination address for which the timer expires, adjust
                //       its ssthresh with rules defined in Section 7.2.3 and set the
                //       cwnd <- MTU.
                // RFC 4960 sec 7.2.3
                //   When the T3-rtx timer expires on an address, SCTP should perform slow
                //   start by:
                //      ssthresh = max(cwnd/2, 4*MTU)
                //      cwnd = 1*MTU

                self.ssthresh = std::cmp::max(self.cwnd / 2, 4 * self.mtu);
                self.cwnd = self.mtu;
                log::trace!(
                    "[{}] updated cwnd={} ssthresh={} inflight={} (RTO)",
                    self.name,
                    self.cwnd,
                    self.ssthresh,
                    self.inflight_queue.get_num_bytes()
                );

                // RFC 3758 sec 3.5
                //  A5) Any time the T3-rtx timer expires, on any destination, the sender
                //  SHOULD try to advance the "Advanced.Peer.Ack.Point" by following
                //  the procedures outlined in C2 - C5.
                if self.use_forward_tsn {
                    // RFC 3758 Sec 3.5 C2
                    let mut i = self.advanced_peer_tsnack_point + 1;
                    while let Some(c) = self.inflight_queue.get(i) {
                        if !c.abandoned() {
                            break;
                        }
                        self.advanced_peer_tsnack_point = i;
                        i += 1;
                    }

                    // RFC 3758 Sec 3.5 C3
                    if sna32gt(
                        self.advanced_peer_tsnack_point,
                        self.cumulative_tsnack_point,
                    ) {
                        self.will_send_forward_tsn = true;
                    }
                }

                log::debug!(
                    "[{}] T3-rtx timed out: n_rtos={} cwnd={} ssthresh={}",
                    self.name,
                    n_rtos,
                    self.cwnd,
                    self.ssthresh
                );

                self.inflight_queue.mark_all_to_retrasmit();
                self.awake_write_loop();
            }

            RtxTimerId::Reconfig => {
                self.will_retransmit_reconfig = true;
                self.awake_write_loop();
            }
        }
    }

    fn on_retransmission_failure(&self, id: RtxTimerId) {
        match id {
            RtxTimerId::T1Init => {
                log::error!("[{}] retransmission failure: T1-init", self.name);
                //TODO: self.handshakeCompletedCh < -errHandshakeInitAck;
            }
            RtxTimerId::T1Cookie => {
                log::error!("[{}] retransmission failure: T1-cookie", self.name);
                //TODO: self.handshakeCompletedCh < -errHandshakeCookieEcho;
            }

            RtxTimerId::T2Shutdown => {
                log::error!("[{}] retransmission failure: T2-shutdown", self.name);
            }

            RtxTimerId::T3RTX => {
                // T3-rtx timer will not fail by design
                // Justifications:
                //  * ICE would fail if the connectivity is lost
                //  * WebRTC spec is not clear how this incident should be reported to ULP
                log::error!("[{}] retransmission failure: T3-rtx (DATA)", self.name);
            }
            _ => {}
        }
    }

    fn on_ack_timeout(&mut self) {
        log::trace!(
            "[{}] ack timed out (ack_state: {})",
            self.name,
            self.ack_state
        );
        self.stats.inc_ack_timeouts();
        self.ack_state = AckState::Immediate;
        self.awake_write_loop();
    }

    /// buffered_amount returns total amount (in bytes) of currently buffered user data.
    /// This is used only by testing.
    fn buffered_amount(&self) -> usize {
        self.pending_queue.get_num_bytes() + self.inflight_queue.get_num_bytes()
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
}
