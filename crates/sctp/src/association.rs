use crate::association_stats::AssociationStats;
use crate::chunk::chunk_cookie_echo::ChunkCookieEcho;
use crate::chunk::chunk_init::ChunkInit;
use crate::chunk::chunk_reconfig::ChunkReconfig;
use crate::error::Error;
use crate::param::param_outgoing_reset_request::ParamOutgoingResetRequest;
use crate::param::param_state_cookie::ParamStateCookie;
use crate::queue::control_queue::ControlQueue;
use crate::queue::payload_queue::PayloadQueue;
use crate::queue::pending_queue::PendingQueue;
use crate::timer::ack_timer::AckTimer;
use crate::timer::rtx_timer::{RtoManager, RtxTimer};

use util::Conn;

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

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
    Closed,
    CookieWait,
    CookieEchoed,
    Established,
    ShutdownAckSent,
    ShutdownPending,
    ShutdownReceived,
    ShutdownSent,
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

/// ack mode (for testing)
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum AckMode {
    Normal,
    NoDelay,
    AlwaysDelay,
}

/// ack transmission state
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum AckState {
    Idle,      // ack timer is off
    Immediate, // will send ack immediately
    Delay,     // ack timer is on (ack is being delayed)
}

/// Config collects the arguments to createAssociation construction into
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
pub struct Association {
    bytes_received: u64,
    bytes_sent: u64,

    //lock sync.RWMutex
    net_conn: Arc<dyn Conn + Send + Sync>,

    peer_verification_tag: u32,
    my_verification_tag: u32,
    state: u32,
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
    max_message_size: u32,
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
    t_reconfig: RtxTimer,
    ack_timer: AckTimer,

    // Chunks stored for retransmission
    stored_init: ChunkInit,
    stored_cookie_echo: ChunkCookieEcho,
    /*TODO:
        streams              map[uint16]*Stream
        acceptCh             chan *Stream
        readLoopCloseCh      chan struct{}
        awakeWriteLoopCh     chan struct{}
        closeWriteLoopCh     chan struct{}
        handshakeCompletedCh chan error
    */
    //TODO: closeWriteLoopOnce sync.Once

    // local error
    silent_error: Error,

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

/*
impl Association {
    /// Server accepts a SCTP stream over a conn
    pub fn Server(config Config) (*Association, error) {
        a := createAssociation(config)
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
        a := createAssociation(config)
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
    }
}

func createAssociation(config Config) *Association {
    var max_receive_buffer_size uint32
    if config.max_receive_buffer_size == 0 {
        max_receive_buffer_size = INITIAL_RECV_BUF_SIZE
    } else {
        max_receive_buffer_size = config.max_receive_buffer_size
    }

    var max_message_size uint32
    if config.max_message_size == 0 {
        max_message_size = DEFAULT_MAX_MESSAGE_SIZE
    } else {
        max_message_size = config.max_message_size
    }

    tsn := globalMathRandomGenerator.Uint32()
    a := &Association{
        net_conn:                 config.net_conn,
        max_receive_buffer_size:    max_receive_buffer_size,
        max_message_size:          max_message_size,
        my_max_num_outbound_streams: math.MaxUint16,
        my_max_num_inbound_streams:  math.MaxUint16,
        payload_queue:            newPayloadQueue(),
        inflight_queue:           newPayloadQueue(),
        pending_queue:            newPendingQueue(),
        control_queue:            newControlQueue(),
        mtu:                     INITIAL_MTU,
        max_payload_size:          INITIAL_MTU - (COMMON_HEADER_SIZE + DATA_CHUNK_HEADER_SIZE),
        my_verification_tag:       globalMathRandomGenerator.Uint32(),
        my_next_tsn:               tsn,
        my_next_rsn:               tsn,
        min_tsn2measure_rtt:       tsn,
        state:                   closed,
        rto_mgr:                  newRTOManager(),
        streams:                 map[uint16]*Stream{},
        reconfigs:               map[uint32]*chunkReconfig{},
        reconfig_requests:        map[uint32]*paramOutgoingResetRequest{},
        acceptCh:                make(chan *Stream, ACCEPT_CH_SIZE),
        readLoopCloseCh:         make(chan struct{}),
        awakeWriteLoopCh:        make(chan struct{}, 1),
        closeWriteLoopCh:        make(chan struct{}),
        handshakeCompletedCh:    make(chan error),
        cumulative_tsnack_point:   tsn - 1,
        advanced_peer_tsnack_point: tsn - 1,
        silent_error:             errSilentlyDiscard,
        stats:                   &associationStats{},
        log:                     config.LoggerFactory.NewLogger("sctp"),
    }

    a.name = fmt.Sprintf("%p", a)

    // RFC 4690 Sec 7.2.1
    //  o  The initial cwnd before DATA transmission or after a sufficiently
    //     long idle period MUST be set to min(4*MTU, max (2*MTU, 4380
    //     bytes)).
    a.cwnd = min32(4*a.mtu, max32(2*a.mtu, 4380))
    a.log.Tracef("[%s] updated cwnd=%d ssthresh=%d inflight=%d (INI)",
        a.name, a.cwnd, a.ssthresh, a.inflight_queue.getNumBytes())

    a.t1init = newRTXTimer(timerT1Init, a, maxInitRetrans)
    a.t1cookie = newRTXTimer(timerT1Cookie, a, maxInitRetrans)
    a.t2shutdown = newRTXTimer(timerT2Shutdown, a, noMaxRetrans) // retransmit forever
    a.t3rtx = newRTXTimer(timerT3RTX, a, noMaxRetrans)           // retransmit forever
    a.t_reconfig = newRTXTimer(timerReconfig, a, noMaxRetrans)    // retransmit forever
    a.ack_timer = newAckTimer(a)

    return a
}

func (a *Association) init(isClient bool) {
    a.lock.Lock()
    defer a.lock.Unlock()

    go a.readLoop()
    go a.writeLoop()

    if isClient {
        a.setState(CookieWait)
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

// caller must hold a.lock
func (a *Association) sendInit() error {
    a.log.Debugf("[%s] sending INIT", a.name)
    if a.stored_init == nil {
        return errInitNotStoredToSend
    }

    outbound := &packet{}
    outbound.verificationTag = a.peer_verification_tag
    a.source_port = 5000      // Spec??
    a.destination_port = 5000 // Spec??
    outbound.source_port = a.source_port
    outbound.destination_port = a.destination_port

    outbound.chunks = []chunk{a.stored_init}

    a.control_queue.push(outbound)
    a.awakeWriteLoop()

    return nil
}

// caller must hold a.lock
func (a *Association) sendCookieEcho() error {
    if a.stored_cookie_echo == nil {
        return errCookieEchoNotStoredToSend
    }

    a.log.Debugf("[%s] sending COOKIE-ECHO", a.name)

    outbound := &packet{}
    outbound.verificationTag = a.peer_verification_tag
    outbound.source_port = a.source_port
    outbound.destination_port = a.destination_port
    outbound.chunks = []chunk{a.stored_cookie_echo}

    a.control_queue.push(outbound)
    a.awakeWriteLoop()

    return nil
}

// Shutdown initiates the shutdown sequence. The method blocks until the
// shutdown sequence is completed and the connection is closed, or until the
// passed context is done, in which case the context's error is returned.
func (a *Association) Shutdown(ctx context.Context) error {
    a.log.Debugf("[%s] closing association..", a.name)

    state := a.getState()

    if state != Established {
        return fmt.Errorf("%w: shutdown %s", errShutdownNonEstablished, a.name)
    }

    // Attempt a graceful shutdown.
    a.setState(ShutdownPending)

    a.lock.Lock()

    if a.inflight_queue.size() == 0 {
        // No more outstanding, send shutdown.
        a.will_send_shutdown = true
        a.awakeWriteLoop()
        a.setState(ShutdownSent)
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
func (a *Association) Close() error {
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

func (a *Association) close() error {
    a.log.Debugf("[%s] closing association..", a.name)

    a.setState(closed)

    err := a.net_conn.Close()

    a.closeAllTimers()

    // awake writeLoop to exit
    a.closeWriteLoopOnce.Do(func() { close(a.closeWriteLoopCh) })

    return err
}

func (a *Association) closeAllTimers() {
    // Close all retransmission & ack timers
    a.t1init.close()
    a.t1cookie.close()
    a.t2shutdown.close()
    a.t3rtx.close()
    a.t_reconfig.close()
    a.ack_timer.close()
}

func (a *Association) readLoop() {
    var closeErr error
    defer func() {
        // also stop writeLoop, otherwise writeLoop can be leaked
        // if connection is lost when there is no writing packet.
        a.closeWriteLoopOnce.Do(func() { close(a.closeWriteLoopCh) })

        a.lock.Lock()
        for _, s := range a.streams {
            a.unregisterStream(s, closeErr)
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
        n, err := a.net_conn.Read(buffer)
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

func (a *Association) writeLoop() {
    a.log.Debugf("[%s] writeLoop entered", a.name)
    defer a.log.Debugf("[%s] writeLoop exited", a.name)

loop:
    for {
        rawPackets, ok := a.gatherOutbound()

        for _, raw := range rawPackets {
            _, err := a.net_conn.Write(raw)
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
        case <-a.awakeWriteLoopCh:
        case <-a.closeWriteLoopCh:
            break loop
        }
    }

    a.setState(closed)
    a.closeAllTimers()
}

func (a *Association) awakeWriteLoop() {
    select {
    case a.awakeWriteLoopCh <- struct{}{}:
    default:
    }
}

// unregisterStream un-registers a stream from the association
// The caller should hold the association write lock.
func (a *Association) unregisterStream(s *Stream, err error) {
    s.lock.Lock()
    defer s.lock.Unlock()

    delete(a.streams, s.streamIdentifier)
    s.readErr = err
    s.readNotifier.Broadcast()
}

// handleInbound parses incoming raw packets
func (a *Association) handleInbound(raw []byte) error {
    p := &packet{}
    if err := p.unmarshal(raw); err != nil {
        a.log.Warnf("[%s] unable to parse SCTP packet %s", a.name, err)
        return nil
    }

    if err := checkPacket(p); err != nil {
        a.log.Warnf("[%s] failed validating packet %s", a.name, err)
        return nil
    }

    a.handleChunkStart()

    for _, c := range p.chunks {
        if err := a.handleChunk(p, c); err != nil {
            return err
        }
    }

    a.handleChunkEnd()

    return nil
}

// The caller should hold the lock
func (a *Association) gatherDataPacketsToRetransmit(rawPackets [][]byte) [][]byte {
    for _, p := range a.getDataPacketsToRetransmit() {
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
func (a *Association) gatherOutboundDataAndReconfigPackets(rawPackets [][]byte) [][]byte {
    // Pop unsent data chunks from the pending queue to send as much as
    // cwnd and rwnd allow.
    chunks, sisToReset := a.popPendingDataChunksToSend()
    if len(chunks) > 0 {
        // Start timer. (noop if already started)
        a.log.Tracef("[%s] T3-rtx timer start (pt1)", a.name)
        a.t3rtx.start(a.rto_mgr.getRTO())
        for _, p := range a.bundleDataChunksIntoPackets(chunks) {
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
                p := a.createPacket([]chunk{c})
                raw, err := p.marshal()
                if err != nil {
                    a.log.Warnf("[%s] failed to serialize a RECONFIG packet to be retransmitted", a.name)
                } else {
                    rawPackets = append(rawPackets, raw)
                }
            }
        }

        if len(sisToReset) > 0 {
            rsn := a.generateNextRSN()
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
            p := a.createPacket([]chunk{c})
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
func (a *Association) gatherOutboundFastRetransmissionPackets(rawPackets [][]byte) [][]byte {
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
            a.checkPartialReliabilityStatus(c)
            toFastRetrans = append(toFastRetrans, c)
            a.log.Tracef("[%s] fast-retransmit: tsn=%d sent=%d htna=%d",
                a.name, c.tsn, c.nSent, a.fast_recover_exit_point)
        }

        if len(toFastRetrans) > 0 {
            raw, err := a.createPacket(toFastRetrans).marshal()
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
func (a *Association) gatherOutboundSackPackets(rawPackets [][]byte) [][]byte {
    if a.ack_state == ackStateImmediate {
        a.ack_state = ackStateIdle
        sack := a.createSelectiveAckChunk()
        a.log.Debugf("[%s] sending SACK: %s", a.name, sack.String())
        raw, err := a.createPacket([]chunk{sack}).marshal()
        if err != nil {
            a.log.Warnf("[%s] failed to serialize a SACK packet", a.name)
        } else {
            rawPackets = append(rawPackets, raw)
        }
    }

    return rawPackets
}

// The caller should hold the lock
func (a *Association) gatherOutboundForwardTSNPackets(rawPackets [][]byte) [][]byte {
    if a.will_send_forward_tsn {
        a.will_send_forward_tsn = false
        if sna32GT(a.advanced_peer_tsnack_point, a.cumulative_tsnack_point) {
            fwdtsn := a.createForwardTSN()
            raw, err := a.createPacket([]chunk{fwdtsn}).marshal()
            if err != nil {
                a.log.Warnf("[%s] failed to serialize a Forward TSN packet", a.name)
            } else {
                rawPackets = append(rawPackets, raw)
            }
        }
    }

    return rawPackets
}

func (a *Association) gatherOutboundShutdownPackets(rawPackets [][]byte) ([][]byte, bool) {
    ok := true

    switch {
    case a.will_send_shutdown:
        a.will_send_shutdown = false

        shutdown := &chunkShutdown{
            cumulativeTSNAck: a.cumulative_tsnack_point,
        }

        raw, err := a.createPacket([]chunk{shutdown}).marshal()
        if err != nil {
            a.log.Warnf("[%s] failed to serialize a Shutdown packet", a.name)
        } else {
            a.t2shutdown.start(a.rto_mgr.getRTO())
            rawPackets = append(rawPackets, raw)
        }
    case a.will_send_shutdown_ack:
        a.will_send_shutdown_ack = false

        shutdownAck := &chunkShutdownAck{}

        raw, err := a.createPacket([]chunk{shutdownAck}).marshal()
        if err != nil {
            a.log.Warnf("[%s] failed to serialize a ShutdownAck packet", a.name)
        } else {
            a.t2shutdown.start(a.rto_mgr.getRTO())
            rawPackets = append(rawPackets, raw)
        }
    case a.will_send_shutdown_complete:
        a.will_send_shutdown_complete = false

        shutdownComplete := &chunkShutdownComplete{}

        raw, err := a.createPacket([]chunk{shutdownComplete}).marshal()
        if err != nil {
            a.log.Warnf("[%s] failed to serialize a ShutdownComplete packet", a.name)
        } else {
            rawPackets = append(rawPackets, raw)
            ok = false
        }
    }

    return rawPackets, ok
}

// gatherOutbound gathers outgoing packets. The returned bool value set to
// false means the association should be closed down after the final send.
func (a *Association) gatherOutbound() ([][]byte, bool) {
    a.lock.Lock()
    defer a.lock.Unlock()

    rawPackets := [][]byte{}

    if a.control_queue.size() > 0 {
        for _, p := range a.control_queue.popAll() {
            raw, err := p.marshal()
            if err != nil {
                a.log.Warnf("[%s] failed to serialize a control packet", a.name)
                continue
            }
            rawPackets = append(rawPackets, raw)
        }
    }

    state := a.getState()

    ok := true

    switch state {
    case Established:
        rawPackets = a.gatherDataPacketsToRetransmit(rawPackets)
        rawPackets = a.gatherOutboundDataAndReconfigPackets(rawPackets)
        rawPackets = a.gatherOutboundFastRetransmissionPackets(rawPackets)
        rawPackets = a.gatherOutboundSackPackets(rawPackets)
        rawPackets = a.gatherOutboundForwardTSNPackets(rawPackets)
    case ShutdownPending, ShutdownSent, ShutdownReceived:
        rawPackets = a.gatherDataPacketsToRetransmit(rawPackets)
        rawPackets = a.gatherOutboundFastRetransmissionPackets(rawPackets)
        rawPackets = a.gatherOutboundSackPackets(rawPackets)
        rawPackets, ok = a.gatherOutboundShutdownPackets(rawPackets)
    case ShutdownAckSent:
        rawPackets, ok = a.gatherOutboundShutdownPackets(rawPackets)
    }

    return rawPackets, ok
}

func checkPacket(p *packet) error {
    // All packets must adhere to these rules

    // This is the SCTP sender's port number.  It can be used by the
    // receiver in combination with the source IP address, the SCTP
    // destination port, and possibly the destination IP address to
    // identify the association to which this packet belongs.  The port
    // number 0 MUST NOT be used.
    if p.source_port == 0 {
        return errSCTPPacketSourcePortZero
    }

    // This is the SCTP port number to which this packet is destined.
    // The receiving host will use this port number to de-multiplex the
    // SCTP packet to the correct receiving endpoint/application.  The
    // port number 0 MUST NOT be used.
    if p.destination_port == 0 {
        return errSCTPPacketDestinationPortZero
    }

    // Check values on the packet that are specific to a particular chunk type
    for _, c := range p.chunks {
        switch c.(type) { // nolint:gocritic
        case *chunkInit:
            // An INIT or INIT ACK chunk MUST NOT be bundled with any other chunk.
            // They MUST be the only chunks present in the SCTP packets that carry
            // them.
            if len(p.chunks) != 1 {
                return errInitChunkBundled
            }

            // A packet containing an INIT chunk MUST have a zero Verification
            // Tag.
            if p.verificationTag != 0 {
                return errInitChunkVerifyTagNotZero
            }
        }
    }

    return nil
}

// setState atomically sets the state of the Association.
// The caller should hold the lock.
func (a *Association) setState(newState uint32) {
    oldState := atomic.SwapUint32(&a.state, newState)
    if newState != oldState {
        a.log.Debugf("[%s] state change: '%s' => '%s'",
            a.name,
            getAssociationStateString(oldState),
            getAssociationStateString(newState))
    }
}

// getState atomically returns the state of the Association.
func (a *Association) getState() uint32 {
    return atomic.LoadUint32(&a.state)
}

// BytesSent returns the number of bytes sent
func (a *Association) BytesSent() uint64 {
    return atomic.LoadUint64(&a.bytes_sent)
}

// BytesReceived returns the number of bytes received
func (a *Association) BytesReceived() uint64 {
    return atomic.LoadUint64(&a.bytes_received)
}

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
func (a *Association) handleInit(p *packet, i *chunkInit) ([]*packet, error) {
    state := a.getState()
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
func (a *Association) handleInitAck(p *packet, i *chunkInitAck) error {
    state := a.getState()
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

    err := a.sendCookieEcho()
    if err != nil {
        a.log.Errorf("[%s] failed to send init: %s", a.name, err.Error())
    }

    a.t1cookie.start(a.rto_mgr.getRTO())
    a.setState(CookieEchoed)
    return nil
}

// The caller should hold the lock.
func (a *Association) handleHeartbeat(c *chunkHeartbeat) []*packet {
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
func (a *Association) handleCookieEcho(c *chunkCookieEcho) []*packet {
    state := a.getState()
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

        a.setState(Established)
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
func (a *Association) handleCookieAck() {
    state := a.getState()
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

    a.setState(Established)
    a.handshakeCompletedCh <- nil
}

// The caller should hold the lock.
func (a *Association) handleData(d *chunkPayloadData) []*packet {
    a.log.Tracef("[%s] DATA: tsn=%d immediateSack=%v len=%d",
        a.name, d.tsn, d.immediateSack, len(d.userData))
    a.stats.inc_datas()

    canPush := a.payload_queue.canPush(d, a.peer_last_tsn)
    if canPush {
        s := a.getOrCreateStream(d.streamIdentifier)
        if s == nil {
            // silentely discard the data. (sender will retry on T3-rtx timeout)
            // see pion/sctp#30
            a.log.Debugf("discard %d", d.streamSequenceNumber)
            return nil
        }

        if a.getMyReceiverWindowCredit() > 0 {
            // Pass the new chunk to stream level as soon as it arrives
            a.payload_queue.push(d, a.peer_last_tsn)
            s.handleData(d)
        } else {
            // Receive buffer is full
            lastTSN, ok := a.payload_queue.getLastTSNReceived()
            if ok && sna32LT(d.tsn, lastTSN) {
                a.log.Debugf("[%s] receive buffer full, but accepted as this is a missing chunk with tsn=%d ssn=%d", a.name, d.tsn, d.streamSequenceNumber)
                a.payload_queue.push(d, a.peer_last_tsn)
                s.handleData(d)
            } else {
                a.log.Debugf("[%s] receive buffer full. dropping DATA with tsn=%d ssn=%d", a.name, d.tsn, d.streamSequenceNumber)
            }
        }
    }

    return a.handlePeerLastTSNAndAcknowledgement(d.immediateSack)
}

// A common routine for handleData and handleForwardTSN routines
// The caller should hold the lock.
func (a *Association) handlePeerLastTSNAndAcknowledgement(sackImmediately bool) []*packet {
    var reply []*packet

    // Try to advance peer_last_tsn

    // From RFC 3758 Sec 3.6:
    //   .. and then MUST further advance its cumulative TSN point locally
    //   if possible
    // Meaning, if peer_last_tsn+1 points to a chunk that is received,
    // advance peer_last_tsn until peer_last_tsn+1 points to unreceived chunk.
    for {
        if _, popOk := a.payload_queue.pop(a.peer_last_tsn + 1); !popOk {
            break
        }
        a.peer_last_tsn++

        for _, rstReq := range a.reconfig_requests {
            resp := a.resetStreamsIfAny(rstReq)
            if resp != nil {
                a.log.Debugf("[%s] RESET RESPONSE: %+v", a.name, resp)
                reply = append(reply, resp)
            }
        }
    }

    hasPacketLoss := (a.payload_queue.size() > 0)
    if hasPacketLoss {
        a.log.Tracef("[%s] packetloss: %s", a.name, a.payload_queue.getGapAckBlocksString(a.peer_last_tsn))
    }

    if (a.ack_state != ackStateImmediate && !sackImmediately && !hasPacketLoss && a.ack_mode == ackModeNormal) || a.ack_mode == ackModeAlwaysDelay {
        if a.ack_state == ackStateIdle {
            a.delayed_ack_triggered = true
        } else {
            a.immediate_ack_triggered = true
        }
    } else {
        a.immediate_ack_triggered = true
    }

    return reply
}

// The caller should hold the lock.
func (a *Association) getMyReceiverWindowCredit() uint32 {
    var bytesQueued uint32
    for _, s := range a.streams {
        bytesQueued += uint32(s.getNumBytesInReassemblyQueue())
    }

    if bytesQueued >= a.max_receive_buffer_size {
        return 0
    }
    return a.max_receive_buffer_size - bytesQueued
}

// OpenStream opens a stream
func (a *Association) OpenStream(streamIdentifier uint16, defaultPayloadType PayloadProtocolIdentifier) (*Stream, error) {
    a.lock.Lock()
    defer a.lock.Unlock()

    if _, ok := a.streams[streamIdentifier]; ok {
        return nil, fmt.Errorf("%w: %d", errStreamAlreadyExist, streamIdentifier)
    }

    s := a.createStream(streamIdentifier, false)
    s.setDefaultPayloadType(defaultPayloadType)

    return s, nil
}

// AcceptStream accepts a stream
func (a *Association) AcceptStream() (*Stream, error) {
    s, ok := <-a.acceptCh
    if !ok {
        return nil, io.EOF // no more incoming streams
    }
    return s, nil
}

// createStream creates a stream. The caller should hold the lock and check no stream exists for this id.
func (a *Association) createStream(streamIdentifier uint16, accept bool) *Stream {
    s := &Stream{
        association:      a,
        streamIdentifier: streamIdentifier,
        reassemblyQueue:  newReassemblyQueue(streamIdentifier),
        log:              a.log,
        name:             fmt.Sprintf("%d:%s", streamIdentifier, a.name),
    }

    s.readNotifier = sync.NewCond(&s.lock)

    if accept {
        select {
        case a.acceptCh <- s:
            a.streams[streamIdentifier] = s
            a.log.Debugf("[%s] accepted a new stream (streamIdentifier: %d)",
                a.name, streamIdentifier)
        default:
            a.log.Debugf("[%s] dropped a new stream (acceptCh size: %d)",
                a.name, len(a.acceptCh))
            return nil
        }
    } else {
        a.streams[streamIdentifier] = s
    }

    return s
}

// getOrCreateStream gets or creates a stream. The caller should hold the lock.
func (a *Association) getOrCreateStream(streamIdentifier uint16) *Stream {
    if s, ok := a.streams[streamIdentifier]; ok {
        return s
    }

    return a.createStream(streamIdentifier, true)
}

// The caller should hold the lock.
func (a *Association) processSelectiveAck(d *chunkSelectiveAck) (map[uint16]int, uint32, error) { // nolint:gocognit
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
func (a *Association) onCumulativeTSNAckPointAdvanced(totalBytesAcked int) {
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
func (a *Association) processFastRetransmission(cumTSNAckPoint, htna uint32, cumTSNAckPointAdvanced bool) error {
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
func (a *Association) handleSack(d *chunkSelectiveAck) error {
    a.log.Tracef("[%s] SACK: cumTSN=%d a_rwnd=%d", a.name, d.cumulativeTSNAck, d.advertisedReceiverWindowCredit)
    state := a.getState()
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
        a.awakeWriteLoop()
    }

    a.postprocessSack(state, cumTSNAckPointAdvanced)

    return nil
}

// The caller must hold the lock. This method was only added because the
// linter was complaining about the "cognitive complexity" of handleSack.
func (a *Association) postprocessSack(state uint32, shouldAwakeWriteLoop bool) {
    switch {
    case a.inflight_queue.size() > 0:
        // Start timer. (noop if already started)
        a.log.Tracef("[%s] T3-rtx timer start (pt3)", a.name)
        a.t3rtx.start(a.rto_mgr.getRTO())
    case state == ShutdownPending:
        // No more outstanding, send shutdown.
        shouldAwakeWriteLoop = true
        a.will_send_shutdown = true
        a.setState(ShutdownSent)
    case state == ShutdownReceived:
        // No more outstanding, send shutdown ack.
        shouldAwakeWriteLoop = true
        a.will_send_shutdown_ack = true
        a.setState(ShutdownAckSent)
    }

    if shouldAwakeWriteLoop {
        a.awakeWriteLoop()
    }
}

// The caller should hold the lock.
func (a *Association) handleShutdown(_ *chunkShutdown) {
    state := a.getState()

    switch state {
    case Established:
        if a.inflight_queue.size() > 0 {
            a.setState(ShutdownReceived)
        } else {
            // No more outstanding, send shutdown ack.
            a.will_send_shutdown_ack = true
            a.setState(ShutdownAckSent)

            a.awakeWriteLoop()
        }

        // a.cumulative_tsnack_point = c.cumulativeTSNAck
    case ShutdownSent:
        a.will_send_shutdown_ack = true
        a.setState(ShutdownAckSent)

        a.awakeWriteLoop()
    }
}

// The caller should hold the lock.
func (a *Association) handleShutdownAck(_ *chunkShutdownAck) {
    state := a.getState()
    if state == ShutdownSent || state == ShutdownAckSent {
        a.t2shutdown.stop()
        a.will_send_shutdown_complete = true

        a.awakeWriteLoop()
    }
}

func (a *Association) handleShutdownComplete(_ *chunkShutdownComplete) error {
    state := a.getState()
    if state == ShutdownAckSent {
        a.t2shutdown.stop()

        return a.close()
    }

    return nil
}

// createForwardTSN generates ForwardTSN chunk.
// This method will be be called if use_forward_tsn is set to false.
// The caller should hold the lock.
func (a *Association) createForwardTSN() *chunkForwardTSN {
    // RFC 3758 Sec 3.5 C4
    streamMap := map[uint16]uint16{} // to report only once per SI
    for i := a.cumulative_tsnack_point + 1; sna32LTE(i, a.advanced_peer_tsnack_point); i++ {
        c, ok := a.inflight_queue.get(i)
        if !ok {
            break
        }

        ssn, ok := streamMap[c.streamIdentifier]
        if !ok {
            streamMap[c.streamIdentifier] = c.streamSequenceNumber
        } else if sna16LT(ssn, c.streamSequenceNumber) {
            // to report only once with greatest SSN
            streamMap[c.streamIdentifier] = c.streamSequenceNumber
        }
    }

    fwdtsn := &chunkForwardTSN{
        newCumulativeTSN: a.advanced_peer_tsnack_point,
        streams:          []chunkForwardTSNStream{},
    }

    var streamStr string
    for si, ssn := range streamMap {
        streamStr += fmt.Sprintf("(si=%d ssn=%d)", si, ssn)
        fwdtsn.streams = append(fwdtsn.streams, chunkForwardTSNStream{
            identifier: si,
            sequence:   ssn,
        })
    }
    a.log.Tracef("[%s] building fwdtsn: newCumulativeTSN=%d cumTSN=%d - %s", a.name, fwdtsn.newCumulativeTSN, a.cumulative_tsnack_point, streamStr)

    return fwdtsn
}

// createPacket wraps chunks in a packet.
// The caller should hold the read lock.
func (a *Association) createPacket(cs []chunk) *packet {
    return &packet{
        verificationTag: a.peer_verification_tag,
        source_port:      a.source_port,
        destination_port: a.destination_port,
        chunks:          cs,
    }
}

// The caller should hold the lock.
func (a *Association) handleReconfig(c *chunkReconfig) ([]*packet, error) {
    a.log.Tracef("[%s] handleReconfig", a.name)

    pp := make([]*packet, 0)

    p, err := a.handleReconfigParam(c.paramA)
    if err != nil {
        return nil, err
    }
    if p != nil {
        pp = append(pp, p)
    }

    if c.paramB != nil {
        p, err = a.handleReconfigParam(c.paramB)
        if err != nil {
            return nil, err
        }
        if p != nil {
            pp = append(pp, p)
        }
    }
    return pp, nil
}

// The caller should hold the lock.
func (a *Association) handleForwardTSN(c *chunkForwardTSN) []*packet {
    a.log.Tracef("[%s] FwdTSN: %s", a.name, c.String())

    if !a.use_forward_tsn {
        a.log.Warn("[%s] received FwdTSN but not enabled")
        // Return an error chunk
        cerr := &chunkError{
            errorCauses: []errorCause{&errorCauseUnrecognizedChunkType{}},
        }
        outbound := &packet{}
        outbound.verificationTag = a.peer_verification_tag
        outbound.source_port = a.source_port
        outbound.destination_port = a.destination_port
        outbound.chunks = []chunk{cerr}
        return []*packet{outbound}
    }

    // From RFC 3758 Sec 3.6:
    //   Note, if the "New Cumulative TSN" value carried in the arrived
    //   FORWARD TSN chunk is found to be behind or at the current cumulative
    //   TSN point, the data receiver MUST treat this FORWARD TSN as out-of-
    //   date and MUST NOT update its Cumulative TSN.  The receiver SHOULD
    //   send a SACK to its peer (the sender of the FORWARD TSN) since such a
    //   duplicate may indicate the previous SACK was lost in the network.

    a.log.Tracef("[%s] should send ack? newCumTSN=%d peer_last_tsn=%d\n",
        a.name, c.newCumulativeTSN, a.peer_last_tsn)
    if sna32LTE(c.newCumulativeTSN, a.peer_last_tsn) {
        a.log.Tracef("[%s] sending ack on Forward TSN", a.name)
        a.ack_state = ackStateImmediate
        a.ack_timer.stop()
        a.awakeWriteLoop()
        return nil
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
    for sna32LT(a.peer_last_tsn, c.newCumulativeTSN) {
        a.payload_queue.pop(a.peer_last_tsn + 1) // may not exist
        a.peer_last_tsn++
    }

    // Report new peer_last_tsn value and abandoned largest SSN value to
    // corresponding streams so that the abandoned chunks can be removed
    // from the reassemblyQueue.
    for _, forwarded := range c.streams {
        if s, ok := a.streams[forwarded.identifier]; ok {
            s.handleForwardTSNForOrdered(forwarded.sequence)
        }
    }

    // TSN may be forewared for unordered chunks. ForwardTSN chunk does not
    // report which stream identifier it skipped for unordered chunks.
    // Therefore, we need to broadcast this event to all existing streams for
    // unordered chunks.
    // See https://github.com/pion/sctp/issues/106
    for _, s := range a.streams {
        s.handleForwardTSNForUnordered(c.newCumulativeTSN)
    }

    return a.handlePeerLastTSNAndAcknowledgement(false)
}

func (a *Association) sendResetRequest(streamIdentifier uint16) error {
    a.lock.Lock()
    defer a.lock.Unlock()

    state := a.getState()
    if state != Established {
        return fmt.Errorf("%w: state=%s", errResetPacketInStateNotExist,
            getAssociationStateString(state))
    }

    // Create DATA chunk which only contains valid stream identifier with
    // nil userData and use it as a EOS from the stream.
    c := &chunkPayloadData{
        streamIdentifier:  streamIdentifier,
        beginningFragment: true,
        endingFragment:    true,
        userData:          nil,
    }

    a.pending_queue.push(c)
    a.awakeWriteLoop()
    return nil
}

// The caller should hold the lock.
func (a *Association) handleReconfigParam(raw param) (*packet, error) {
    switch p := raw.(type) {
    case *paramOutgoingResetRequest:
        a.reconfig_requests[p.reconfigRequestSequenceNumber] = p
        resp := a.resetStreamsIfAny(p)
        if resp != nil {
            return resp, nil
        }
        return nil, nil

    case *paramReconfigResponse:
        delete(a.reconfigs, p.reconfigResponseSequenceNumber)
        if len(a.reconfigs) == 0 {
            a.t_reconfig.stop()
        }
        return nil, nil
    default:
        return nil, fmt.Errorf("%w: %t", errParamterType, p)
    }
}

// The caller should hold the lock.
func (a *Association) resetStreamsIfAny(p *paramOutgoingResetRequest) *packet {
    result := reconfigResultSuccessPerformed
    if sna32LTE(p.senderLastTSN, a.peer_last_tsn) {
        a.log.Debugf("[%s] resetStream(): senderLastTSN=%d <= peer_last_tsn=%d",
            a.name, p.senderLastTSN, a.peer_last_tsn)
        for _, id := range p.streamIdentifiers {
            s, ok := a.streams[id]
            if !ok {
                continue
            }
            a.unregisterStream(s, io.EOF)
        }
        delete(a.reconfig_requests, p.reconfigRequestSequenceNumber)
    } else {
        a.log.Debugf("[%s] resetStream(): senderLastTSN=%d > peer_last_tsn=%d",
            a.name, p.senderLastTSN, a.peer_last_tsn)
        result = reconfigResultInProgress
    }

    return a.createPacket([]chunk{&chunkReconfig{
        paramA: &paramReconfigResponse{
            reconfigResponseSequenceNumber: p.reconfigRequestSequenceNumber,
            result:                         result,
        },
    }})
}

// Move the chunk peeked with a.pending_queue.peek() to the inflight_queue.
// The caller should hold the lock.
func (a *Association) movePendingDataChunkToInflightQueue(c *chunkPayloadData) {
    if err := a.pending_queue.pop(c); err != nil {
        a.log.Errorf("[%s] failed to pop from pending queue: %s", a.name, err.Error())
    }

    // Mark all fragements are in-flight now
    if c.endingFragment {
        c.setAllInflight()
    }

    // Assign TSN
    c.tsn = a.generateNextTSN()

    c.since = time.Now() // use to calculate RTT and also for maxPacketLifeTime
    c.nSent = 1          // being sent for the first time

    a.checkPartialReliabilityStatus(c)

    a.log.Tracef("[%s] sending ppi=%d tsn=%d ssn=%d sent=%d len=%d (%v,%v)",
        a.name, c.payloadType, c.tsn, c.streamSequenceNumber, c.nSent, len(c.userData), c.beginningFragment, c.endingFragment)

    a.inflight_queue.pushNoCheck(c)
}

// popPendingDataChunksToSend pops chunks from the pending queues as many as
// the cwnd and rwnd allows to send.
// The caller should hold the lock.
func (a *Association) popPendingDataChunksToSend() ([]*chunkPayloadData, []uint16) {
    chunks := []*chunkPayloadData{}
    var sisToReset []uint16 // stream identifieres to reset

    if a.pending_queue.size() > 0 {
        // RFC 4960 sec 6.1.  Transmission of DATA Chunks
        //   A) At any given time, the data sender MUST NOT transmit new data to
        //      any destination transport address if its peer's rwnd indicates
        //      that the peer has no buffer space (i.e., rwnd is 0; see Section
        //      6.2.1).  However, regardless of the value of rwnd (including if it
        //      is 0), the data sender can always have one DATA chunk in flight to
        //      the receiver if allowed by cwnd (see rule B, below).

        for {
            c := a.pending_queue.peek()
            if c == nil {
                break // no more pending data
            }

            dataLen := uint32(len(c.userData))
            if dataLen == 0 {
                sisToReset = append(sisToReset, c.streamIdentifier)
                err := a.pending_queue.pop(c)
                if err != nil {
                    a.log.Errorf("failed to pop from pending queue: %s", err.Error())
                }
                continue
            }

            if uint32(a.inflight_queue.getNumBytes())+dataLen > a.cwnd {
                break // would exceeds cwnd
            }

            if dataLen > a.rwnd {
                break // no more rwnd
            }

            a.rwnd -= dataLen

            a.movePendingDataChunkToInflightQueue(c)
            chunks = append(chunks, c)
        }

        // the data sender can always have one DATA chunk in flight to the receiver
        if len(chunks) == 0 && a.inflight_queue.size() == 0 {
            // Send zero window probe
            c := a.pending_queue.peek()
            if c != nil {
                a.movePendingDataChunkToInflightQueue(c)
                chunks = append(chunks, c)
            }
        }
    }

    return chunks, sisToReset
}

// bundleDataChunksIntoPackets packs DATA chunks into packets. It tries to bundle
// DATA chunks into a packet so long as the resulting packet size does not exceed
// the path MTU.
// The caller should hold the lock.
func (a *Association) bundleDataChunksIntoPackets(chunks []*chunkPayloadData) []*packet {
    packets := []*packet{}
    chunksToSend := []chunk{}
    bytesInPacket := int(COMMON_HEADER_SIZE)

    for _, c := range chunks {
        // RFC 4960 sec 6.1.  Transmission of DATA Chunks
        //   Multiple DATA chunks committed for transmission MAY be bundled in a
        //   single packet.  Furthermore, DATA chunks being retransmitted MAY be
        //   bundled with new DATA chunks, as long as the resulting packet size
        //   does not exceed the path MTU.
        if bytesInPacket+len(c.userData) > int(a.mtu) {
            packets = append(packets, a.createPacket(chunksToSend))
            chunksToSend = []chunk{}
            bytesInPacket = int(COMMON_HEADER_SIZE)
        }

        chunksToSend = append(chunksToSend, c)
        bytesInPacket += int(DATA_CHUNK_HEADER_SIZE) + len(c.userData)
    }

    if len(chunksToSend) > 0 {
        packets = append(packets, a.createPacket(chunksToSend))
    }

    return packets
}

// sendPayloadData sends the data chunks.
func (a *Association) sendPayloadData(chunks []*chunkPayloadData) error {
    a.lock.Lock()
    defer a.lock.Unlock()

    state := a.getState()
    if state != Established {
        return fmt.Errorf("%w: state=%s", errPayloadDataStateNotExist,
            getAssociationStateString(state))
    }

    // Push the chunks into the pending queue first.
    for _, c := range chunks {
        a.pending_queue.push(c)
    }

    a.awakeWriteLoop()
    return nil
}

// The caller should hold the lock.
func (a *Association) checkPartialReliabilityStatus(c *chunkPayloadData) {
    if !a.use_forward_tsn {
        return
    }

    // draft-ietf-rtcweb-data-protocol-09.txt section 6
    //	6.  Procedures
    //		All Data Channel Establishment Protocol messages MUST be sent using
    //		ordered delivery and reliable transmission.
    //
    if c.payloadType == PayloadTypeWebRTCDCEP {
        return
    }

    // PR-SCTP
    if s, ok := a.streams[c.streamIdentifier]; ok {
        s.lock.RLock()
        if s.reliabilityType == ReliabilityTypeRexmit {
            if c.nSent >= s.reliabilityValue {
                c.setAbandoned(true)
                a.log.Tracef("[%s] marked as abandoned: tsn=%d ppi=%d (remix: %d)", a.name, c.tsn, c.payloadType, c.nSent)
            }
        } else if s.reliabilityType == ReliabilityTypeTimed {
            elapsed := int64(time.Since(c.since).Seconds() * 1000)
            if elapsed >= int64(s.reliabilityValue) {
                c.setAbandoned(true)
                a.log.Tracef("[%s] marked as abandoned: tsn=%d ppi=%d (timed: %d)", a.name, c.tsn, c.payloadType, elapsed)
            }
        }
        s.lock.RUnlock()
    } else {
        a.log.Errorf("[%s] stream %d not found)", a.name, c.streamIdentifier)
    }
}

// getDataPacketsToRetransmit is called when T3-rtx is timed out and retransmit outstanding data chunks
// that are not acked or abandoned yet.
// The caller should hold the lock.
func (a *Association) getDataPacketsToRetransmit() []*packet {
    awnd := min32(a.cwnd, a.rwnd)
    chunks := []*chunkPayloadData{}
    var bytesToSend int
    var done bool

    for i := 0; !done; i++ {
        c, ok := a.inflight_queue.get(a.cumulative_tsnack_point + uint32(i) + 1)
        if !ok {
            break // end of pending data
        }

        if !c.retransmit {
            continue
        }

        if i == 0 && int(a.rwnd) < len(c.userData) {
            // Send it as a zero window probe
            done = true
        } else if bytesToSend+len(c.userData) > int(awnd) {
            break
        }

        // reset the retransmit flag not to retransmit again before the next
        // t3-rtx timer fires
        c.retransmit = false
        bytesToSend += len(c.userData)

        c.nSent++

        a.checkPartialReliabilityStatus(c)

        a.log.Tracef("[%s] retransmitting tsn=%d ssn=%d sent=%d", a.name, c.tsn, c.streamSequenceNumber, c.nSent)

        chunks = append(chunks, c)
    }

    return a.bundleDataChunksIntoPackets(chunks)
}

// generateNextTSN returns the my_next_tsn and increases it. The caller should hold the lock.
// The caller should hold the lock.
func (a *Association) generateNextTSN() uint32 {
    tsn := a.my_next_tsn
    a.my_next_tsn++
    return tsn
}

// generateNextRSN returns the my_next_rsn and increases it. The caller should hold the lock.
// The caller should hold the lock.
func (a *Association) generateNextRSN() uint32 {
    rsn := a.my_next_rsn
    a.my_next_rsn++
    return rsn
}

func (a *Association) createSelectiveAckChunk() *chunkSelectiveAck {
    sack := &chunkSelectiveAck{}
    sack.cumulativeTSNAck = a.peer_last_tsn
    sack.advertisedReceiverWindowCredit = a.getMyReceiverWindowCredit()
    sack.duplicateTSN = a.payload_queue.popDuplicates()
    sack.gapAckBlocks = a.payload_queue.getGapAckBlocks(a.peer_last_tsn)
    return sack
}

func pack(p *packet) []*packet {
    return []*packet{p}
}

func (a *Association) handleChunkStart() {
    a.lock.Lock()
    defer a.lock.Unlock()

    a.delayed_ack_triggered = false
    a.immediate_ack_triggered = false
}

func (a *Association) handleChunkEnd() {
    a.lock.Lock()
    defer a.lock.Unlock()

    if a.immediate_ack_triggered {
        a.ack_state = ackStateImmediate
        a.ack_timer.stop()
        a.awakeWriteLoop()
    } else if a.delayed_ack_triggered {
        // Will send delayed ack in the next ack timeout
        a.ack_state = ackStateDelay
        a.ack_timer.start()
    }
}

func (a *Association) handleChunk(p *packet, c chunk) error {
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
        packets = a.handleData(c)

    case *chunkSelectiveAck:
        err = a.handleSack(c)

    case *chunkReconfig:
        packets, err = a.handleReconfig(c)

    case *chunkForwardTSN:
        packets = a.handleForwardTSN(c)

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
        a.awakeWriteLoop()
    }

    return nil
}

func (a *Association) onRetransmissionTimeout(id int, nRtos uint) {
    a.lock.Lock()
    defer a.lock.Unlock()

    if id == timerT1Init {
        err := a.sendInit()
        if err != nil {
            a.log.Debugf("[%s] failed to retransmit init (nRtos=%d): %v", a.name, nRtos, err)
        }
        return
    }

    if id == timerT1Cookie {
        err := a.sendCookieEcho()
        if err != nil {
            a.log.Debugf("[%s] failed to retransmit cookie-echo (nRtos=%d): %v", a.name, nRtos, err)
        }
        return
    }

    if id == timerT2Shutdown {
        a.log.Debugf("[%s] retransmission of shutdown timeout (nRtos=%d): %v", a.name, nRtos)
        state := a.getState()

        switch state {
        case ShutdownSent:
            a.will_send_shutdown = true
            a.awakeWriteLoop()
        case ShutdownAckSent:
            a.will_send_shutdown_ack = true
            a.awakeWriteLoop()
        }
    }

    if id == timerT3RTX {
        a.stats.inc_t3timeouts()

        // RFC 4960 sec 6.3.3
        //  E1)  For the destination address for which the timer expires, adjust
        //       its ssthresh with rules defined in Section 7.2.3 and set the
        //       cwnd <- MTU.
        // RFC 4960 sec 7.2.3
        //   When the T3-rtx timer expires on an address, SCTP should perform slow
        //   start by:
        //      ssthresh = max(cwnd/2, 4*MTU)
        //      cwnd = 1*MTU

        a.ssthresh = max32(a.cwnd/2, 4*a.mtu)
        a.cwnd = a.mtu
        a.log.Tracef("[%s] updated cwnd=%d ssthresh=%d inflight=%d (RTO)",
            a.name, a.cwnd, a.ssthresh, a.inflight_queue.getNumBytes())

        // RFC 3758 sec 3.5
        //  A5) Any time the T3-rtx timer expires, on any destination, the sender
        //  SHOULD try to advance the "Advanced.Peer.Ack.Point" by following
        //  the procedures outlined in C2 - C5.
        if a.use_forward_tsn {
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
        }

        a.log.Debugf("[%s] T3-rtx timed out: nRtos=%d cwnd=%d ssthresh=%d", a.name, nRtos, a.cwnd, a.ssthresh)

        /*
            a.log.Debugf("   - advanced_peer_tsnack_point=%d", a.advanced_peer_tsnack_point)
            a.log.Debugf("   - cumulative_tsnack_point=%d", a.cumulative_tsnack_point)
            a.inflight_queue.updateSortedKeys()
            for i, tsn := range a.inflight_queue.sorted {
                if c, ok := a.inflight_queue.get(tsn); ok {
                    a.log.Debugf("   - [%d] tsn=%d acked=%v abandoned=%v (%v,%v) len=%d",
                        i, c.tsn, c.acked, c.abandoned(), c.beginningFragment, c.endingFragment, len(c.userData))
                }
            }
        */

        a.inflight_queue.markAllToRetrasmit()
        a.awakeWriteLoop()

        return
    }

    if id == timerReconfig {
        a.will_retransmit_reconfig = true
        a.awakeWriteLoop()
    }
}

func (a *Association) onRetransmissionFailure(id int) {
    a.lock.Lock()
    defer a.lock.Unlock()

    if id == timerT1Init {
        a.log.Errorf("[%s] retransmission failure: T1-init", a.name)
        a.handshakeCompletedCh <- errHandshakeInitAck
        return
    }

    if id == timerT1Cookie {
        a.log.Errorf("[%s] retransmission failure: T1-cookie", a.name)
        a.handshakeCompletedCh <- errHandshakeCookieEcho
        return
    }

    if id == timerT2Shutdown {
        a.log.Errorf("[%s] retransmission failure: T2-shutdown", a.name)
        return
    }

    if id == timerT3RTX {
        // T3-rtx timer will not fail by design
        // Justifications:
        //  * ICE would fail if the connectivity is lost
        //  * WebRTC spec is not clear how this incident should be reported to ULP
        a.log.Errorf("[%s] retransmission failure: T3-rtx (DATA)", a.name)
        return
    }
}

func (a *Association) onAckTimeout() {
    a.lock.Lock()
    defer a.lock.Unlock()

    a.log.Tracef("[%s] ack timed out (ack_state: %d)", a.name, a.ack_state)
    a.stats.inc_ack_timeouts()

    a.ack_state = ackStateImmediate
    a.awakeWriteLoop()
}

// bufferedAmount returns total amount (in bytes) of currently buffered user data.
// This is used only by testing.
func (a *Association) bufferedAmount() int {
    a.lock.RLock()
    defer a.lock.RUnlock()

    return a.pending_queue.getNumBytes() + a.inflight_queue.getNumBytes()
}

// max_message_size returns the maximum message size you can send.
func (a *Association) max_message_size() uint32 {
    return atomic.LoadUint32(&a.max_message_size)
}

// SetMaxMessageSize sets the maximum message size you can send.
func (a *Association) SetMaxMessageSize(maxMsgSize uint32) {
    atomic.StoreUint32(&a.max_message_size, maxMsgSize)
}
*/
