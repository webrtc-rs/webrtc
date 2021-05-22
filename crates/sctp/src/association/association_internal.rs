use super::*;

#[derive(Default)]
pub struct AssociationInternal {
    /*bytes_received: u64,
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
    my_cookie: Option<ParamStateCookie>,
    payload_queue: PayloadQueue,
    inflight_queue: PayloadQueue,
    pending_queue: PendingQueue,
    control_queue: ControlQueue,
    mtu: u32,
    max_payload_size: u32, // max DATA chunk payload size
    cumulative_tsn_ack_point: u32,
    advanced_peer_tsn_ack_point: u32,
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
    //TODO: handshakeCompletedCh : mpsc:: chan error
    //TODO: closeWriteLoopOnce sync.Once

    // local error
    silent_error: Option<Error>,

    // per inbound packet context
    delayed_ack_triggered: bool,
    immediate_ack_triggered: bool,
    */
    name: String,
    awake_write_loop_ch: Arc<Notify>,
    stats: Arc<AssociationStats>,
    ack_state: AckState,
    ack_mode: AckMode, // for testing
}
