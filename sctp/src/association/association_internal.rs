#[cfg(test)]
mod association_internal_test;

use super::*;

use crate::param::param_type::ParamType;
use crate::param::param_unrecognized::ParamUnrecognized;
use async_trait::async_trait;
use std::sync::atomic::AtomicBool;

#[derive(Default)]
pub struct AssociationInternal {
    pub(crate) name: String,
    pub(crate) state: Arc<AtomicU8>,
    pub(crate) max_message_size: Arc<AtomicU32>,
    pub(crate) inflight_queue_length: Arc<AtomicUsize>,
    pub(crate) will_send_shutdown: Arc<AtomicBool>,
    awake_write_loop_ch: Option<Arc<mpsc::Sender<()>>>,

    peer_verification_tag: u32,
    pub(crate) my_verification_tag: u32,

    pub(crate) my_next_tsn: u32, // nextTSN
    peer_last_tsn: u32,          // lastRcvdTSN
    min_tsn2measure_rtt: u32,    // for RTT measurement
    will_send_forward_tsn: bool,
    will_retransmit_fast: bool,
    will_retransmit_reconfig: bool,

    will_send_shutdown_ack: bool,
    will_send_shutdown_complete: bool,

    // Reconfig
    my_next_rsn: u32,
    reconfigs: HashMap<u32, ChunkReconfig>,
    reconfig_requests: HashMap<u32, ParamOutgoingResetRequest>,

    // Non-RFC internal data
    source_port: u16,
    destination_port: u16,
    pub(crate) my_max_num_inbound_streams: u16,
    pub(crate) my_max_num_outbound_streams: u16,
    my_cookie: Option<ParamStateCookie>,
    payload_queue: PayloadQueue,
    inflight_queue: PayloadQueue,
    pending_queue: Arc<PendingQueue>,
    control_queue: ControlQueue,
    pub(crate) mtu: u32,
    max_payload_size: u32, // max DATA chunk payload size
    cumulative_tsn_ack_point: u32,
    advanced_peer_tsn_ack_point: u32,
    use_forward_tsn: bool,

    // Congestion control parameters
    pub(crate) max_receive_buffer_size: u32,
    pub(crate) cwnd: u32,     // my congestion window size
    rwnd: u32,                // calculated peer's receiver windows size
    pub(crate) ssthresh: u32, // slow start threshold
    partial_bytes_acked: u32,
    pub(crate) in_fast_recovery: bool,
    fast_recover_exit_point: u32,

    // RTX & Ack timer
    pub(crate) rto_mgr: RtoManager,
    pub(crate) t1init: Option<RtxTimer<AssociationInternal>>,
    pub(crate) t1cookie: Option<RtxTimer<AssociationInternal>>,
    pub(crate) t2shutdown: Option<RtxTimer<AssociationInternal>>,
    pub(crate) t3rtx: Option<RtxTimer<AssociationInternal>>,
    pub(crate) treconfig: Option<RtxTimer<AssociationInternal>>,
    pub(crate) ack_timer: Option<AckTimer<AssociationInternal>>,

    // Chunks stored for retransmission
    pub(crate) stored_init: Option<ChunkInit>,
    stored_cookie_echo: Option<ChunkCookieEcho>,

    streams: HashMap<u16, Arc<Stream>>,

    close_loop_ch_tx: Option<broadcast::Sender<()>>,
    accept_ch_tx: Option<mpsc::Sender<Arc<Stream>>>,
    handshake_completed_ch_tx: Option<mpsc::Sender<Option<Error>>>,

    // local error
    silent_error: Option<Error>,

    // per inbound packet context
    delayed_ack_triggered: bool,
    immediate_ack_triggered: bool,

    pub(crate) stats: Arc<AssociationStats>,
    ack_state: AckState,
    pub(crate) ack_mode: AckMode, // for testing
}

impl AssociationInternal {
    pub(crate) fn new(
        config: Config,
        close_loop_ch_tx: broadcast::Sender<()>,
        accept_ch_tx: mpsc::Sender<Arc<Stream>>,
        handshake_completed_ch_tx: mpsc::Sender<Option<Error>>,
        awake_write_loop_ch: Arc<mpsc::Sender<()>>,
    ) -> Self {
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

        let inflight_queue_length = Arc::new(AtomicUsize::new(0));

        let mut tsn = random::<u32>();
        if tsn == 0 {
            tsn += 1;
        }
        let mut a = AssociationInternal {
            name: config.name,
            max_receive_buffer_size,
            max_message_size: Arc::new(AtomicU32::new(max_message_size)),

            my_max_num_outbound_streams: u16::MAX,
            my_max_num_inbound_streams: u16::MAX,
            payload_queue: PayloadQueue::new(Arc::new(AtomicUsize::new(0))),
            inflight_queue: PayloadQueue::new(Arc::clone(&inflight_queue_length)),
            inflight_queue_length,
            pending_queue: Arc::new(PendingQueue::new()),
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
            accept_ch_tx: Some(accept_ch_tx),
            close_loop_ch_tx: Some(close_loop_ch_tx),
            handshake_completed_ch_tx: Some(handshake_completed_ch_tx),
            cumulative_tsn_ack_point: tsn - 1,
            advanced_peer_tsn_ack_point: tsn - 1,
            silent_error: Some(Error::ErrSilentlyDiscard),
            stats: Arc::new(AssociationStats::default()),
            awake_write_loop_ch: Some(awake_write_loop_ch),
            ..Default::default()
        };

        // RFC 4690 Sec 7.2.1
        //  o  The initial cwnd before DATA transmission or after a sufficiently
        //     long idle period MUST be set to min(4*MTU, max (2*MTU, 4380
        //     bytes)).
        //     TODO: Consider whether this should use `clamp`
        #[allow(clippy::manual_clamp)]
        {
            a.cwnd = std::cmp::min(4 * a.mtu, std::cmp::max(2 * a.mtu, 4380));
        }
        log::trace!(
            "[{}] updated cwnd={} ssthresh={} inflight={} (INI)",
            a.name,
            a.cwnd,
            a.ssthresh,
            a.inflight_queue.get_num_bytes()
        );

        a
    }

    /// caller must hold self.lock
    pub(crate) fn send_init(&mut self) -> Result<()> {
        if let Some(stored_init) = self.stored_init.clone() {
            log::debug!("[{}] sending INIT", self.name);

            self.source_port = 5000; // Spec??
            self.destination_port = 5000; // Spec??

            let outbound = Packet {
                source_port: self.source_port,
                destination_port: self.destination_port,
                verification_tag: 0,
                chunks: vec![Box::new(stored_init)],
            };

            self.control_queue.push_back(outbound);
            self.awake_write_loop();

            Ok(())
        } else {
            Err(Error::ErrInitNotStoredToSend)
        }
    }

    /// caller must hold self.lock
    fn send_cookie_echo(&mut self) -> Result<()> {
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

    pub(crate) async fn close(&mut self) -> Result<()> {
        if self.get_state() != AssociationState::Closed {
            self.set_state(AssociationState::Closed);

            log::debug!("[{}] closing association..", self.name);

            self.close_all_timers().await;

            // awake read/write_loop to exit
            self.close_loop_ch_tx.take();

            for si in self.streams.keys().cloned().collect::<Vec<u16>>() {
                self.unregister_stream(si);
            }

            // Wait for read_loop to end
            //if let Some(read_loop_close_ch) = &mut self.read_loop_close_ch {
            //    let _ = read_loop_close_ch.recv().await;
            //}

            log::debug!("[{}] association closed", self.name);
            log::debug!(
                "[{}] stats nDATAs (in) : {}",
                self.name,
                self.stats.get_num_datas()
            );
            log::debug!(
                "[{}] stats nSACKs (in) : {}",
                self.name,
                self.stats.get_num_sacks()
            );
            log::debug!(
                "[{}] stats nT3Timeouts : {}",
                self.name,
                self.stats.get_num_t3timeouts()
            );
            log::debug!(
                "[{}] stats nAckTimeouts: {}",
                self.name,
                self.stats.get_num_ack_timeouts()
            );
            log::debug!(
                "[{}] stats nFastRetrans: {}",
                self.name,
                self.stats.get_num_fast_retrans()
            );
        }

        Ok(())
    }

    async fn close_all_timers(&mut self) {
        // Close all retransmission & ack timers
        if let Some(t1init) = &self.t1init {
            t1init.stop().await;
        }
        if let Some(t1cookie) = &self.t1cookie {
            t1cookie.stop().await;
        }
        if let Some(t2shutdown) = &self.t2shutdown {
            t2shutdown.stop().await;
        }
        if let Some(t3rtx) = &self.t3rtx {
            t3rtx.stop().await;
        }
        if let Some(treconfig) = &self.treconfig {
            treconfig.stop().await;
        }
        if let Some(ack_timer) = &mut self.ack_timer {
            ack_timer.stop();
        }
    }

    fn awake_write_loop(&self) {
        //log::debug!("[{}] awake_write_loop_ch.notify_one", self.name);
        if let Some(awake_write_loop_ch) = &self.awake_write_loop_ch {
            let _ = awake_write_loop_ch.try_send(());
        }
    }

    /// unregister_stream un-registers a stream from the association
    /// The caller should hold the association write lock.
    fn unregister_stream(&mut self, stream_identifier: u16) {
        let s = self.streams.remove(&stream_identifier);
        if let Some(s) = s {
            // NOTE: shutdown is not used here because it resets the stream.
            if !s.read_shutdown.swap(true, Ordering::SeqCst) {
                s.read_notifier.notify_waiters();
            }
            s.write_shutdown.store(true, Ordering::SeqCst);
        }
    }

    /// handle_inbound parses incoming raw packets
    pub(crate) async fn handle_inbound(&mut self, raw: &Bytes) -> Result<()> {
        let p = match Packet::unmarshal(raw) {
            Ok(p) => p,
            Err(err) => {
                log::warn!("[{}] unable to parse SCTP packet {}", self.name, err);
                return Ok(());
            }
        };

        if let Err(err) = p.check_packet() {
            log::warn!("[{}] failed validating packet {}", self.name, err);
            return Ok(());
        }

        self.handle_chunk_start();

        for c in &p.chunks {
            self.handle_chunk(&p, c).await?;
        }

        self.handle_chunk_end();
        Ok(())
    }

    fn gather_data_packets_to_retransmit(&mut self, mut raw_packets: Vec<Packet>) -> Vec<Packet> {
        for p in self.get_data_packets_to_retransmit() {
            raw_packets.push(p);
        }

        raw_packets
    }

    async fn gather_outbound_data_and_reconfig_packets(
        &mut self,
        mut raw_packets: Vec<Packet>,
    ) -> Vec<Packet> {
        // Pop unsent data chunks from the pending queue to send as much as
        // cwnd and rwnd allow.
        let (chunks, sis_to_reset) = self.pop_pending_data_chunks_to_send().await;
        if !chunks.is_empty() {
            // Start timer. (noop if already started)
            log::trace!("[{}] T3-rtx timer start (pt1)", self.name);
            if let Some(t3rtx) = &self.t3rtx {
                t3rtx.start(self.rto_mgr.get_rto()).await;
            }
            for p in self.bundle_data_chunks_into_packets(chunks) {
                raw_packets.push(p);
            }
        }

        if !sis_to_reset.is_empty() || self.will_retransmit_reconfig {
            if self.will_retransmit_reconfig {
                self.will_retransmit_reconfig = false;
                log::debug!(
                    "[{}] retransmit {} RECONFIG chunk(s)",
                    self.name,
                    self.reconfigs.len()
                );
                for c in self.reconfigs.values() {
                    let p = self.create_packet(vec![Box::new(c.clone())]);
                    raw_packets.push(p);
                }
            }

            if !sis_to_reset.is_empty() {
                let rsn = self.generate_next_rsn();
                let tsn = self.my_next_tsn - 1;
                log::debug!(
                    "[{}] sending RECONFIG: rsn={} tsn={} streams={:?}",
                    self.name,
                    rsn,
                    self.my_next_tsn - 1,
                    sis_to_reset
                );

                let c = ChunkReconfig {
                    param_a: Some(Box::new(ParamOutgoingResetRequest {
                        reconfig_request_sequence_number: rsn,
                        sender_last_tsn: tsn,
                        stream_identifiers: sis_to_reset,
                        ..Default::default()
                    })),
                    ..Default::default()
                };
                self.reconfigs.insert(rsn, c.clone()); // store in the map for retransmission

                let p = self.create_packet(vec![Box::new(c)]);
                raw_packets.push(p);
            }

            if !self.reconfigs.is_empty() {
                if let Some(treconfig) = &self.treconfig {
                    treconfig.start(self.rto_mgr.get_rto()).await;
                }
            }
        }

        raw_packets
    }

    fn gather_outbound_fast_retransmission_packets(
        &mut self,
        mut raw_packets: Vec<Packet>,
    ) -> Vec<Packet> {
        if self.will_retransmit_fast {
            self.will_retransmit_fast = false;

            let mut to_fast_retrans: Vec<Box<dyn Chunk + Send + Sync>> = vec![];
            let mut fast_retrans_size = COMMON_HEADER_SIZE;

            let mut i = 0;
            loop {
                let tsn = self.cumulative_tsn_ack_point + i + 1;
                if let Some(c) = self.inflight_queue.get_mut(tsn) {
                    if c.acked || c.abandoned() || c.nsent > 1 || c.miss_indicator < 3 {
                        i += 1;
                        continue;
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

                    let data_chunk_size = DATA_CHUNK_HEADER_SIZE + c.user_data.len() as u32;
                    if self.mtu < fast_retrans_size + data_chunk_size {
                        break;
                    }

                    fast_retrans_size += data_chunk_size;
                    self.stats.inc_fast_retrans();
                    c.nsent += 1;
                } else {
                    break; // end of pending data
                }

                if let Some(c) = self.inflight_queue.get(tsn) {
                    self.check_partial_reliability_status(c);
                    to_fast_retrans.push(Box::new(c.clone()));
                    log::trace!(
                        "[{}] fast-retransmit: tsn={} sent={} htna={}",
                        self.name,
                        c.tsn,
                        c.nsent,
                        self.fast_recover_exit_point
                    );
                }
                i += 1;
            }

            if !to_fast_retrans.is_empty() {
                let p = self.create_packet(to_fast_retrans);
                raw_packets.push(p);
            }
        }

        raw_packets
    }

    async fn gather_outbound_sack_packets(&mut self, mut raw_packets: Vec<Packet>) -> Vec<Packet> {
        if self.ack_state == AckState::Immediate {
            self.ack_state = AckState::Idle;
            let sack = self.create_selective_ack_chunk().await;
            log::debug!("[{}] sending SACK: {}", self.name, sack);
            let p = self.create_packet(vec![Box::new(sack)]);
            raw_packets.push(p);
        }

        raw_packets
    }

    fn gather_outbound_forward_tsn_packets(&mut self, mut raw_packets: Vec<Packet>) -> Vec<Packet> {
        /*log::debug!(
            "[{}] gatherOutboundForwardTSNPackets {}",
            self.name,
            self.will_send_forward_tsn
        );*/
        if self.will_send_forward_tsn {
            self.will_send_forward_tsn = false;
            if sna32gt(
                self.advanced_peer_tsn_ack_point,
                self.cumulative_tsn_ack_point,
            ) {
                let fwd_tsn = self.create_forward_tsn();
                let p = self.create_packet(vec![Box::new(fwd_tsn)]);
                raw_packets.push(p);
            }
        }

        raw_packets
    }

    async fn gather_outbound_shutdown_packets(
        &mut self,
        mut raw_packets: Vec<Packet>,
    ) -> (Vec<Packet>, bool) {
        let mut ok = true;

        if self.will_send_shutdown.load(Ordering::SeqCst) {
            self.will_send_shutdown.store(false, Ordering::SeqCst);

            let shutdown = ChunkShutdown {
                cumulative_tsn_ack: self.cumulative_tsn_ack_point,
            };

            let p = self.create_packet(vec![Box::new(shutdown)]);
            if let Some(t2shutdown) = &self.t2shutdown {
                t2shutdown.start(self.rto_mgr.get_rto()).await;
            }
            raw_packets.push(p);
        } else if self.will_send_shutdown_ack {
            self.will_send_shutdown_ack = false;

            let shutdown_ack = ChunkShutdownAck {};

            let p = self.create_packet(vec![Box::new(shutdown_ack)]);
            if let Some(t2shutdown) = &self.t2shutdown {
                t2shutdown.start(self.rto_mgr.get_rto()).await;
            }
            raw_packets.push(p);
        } else if self.will_send_shutdown_complete {
            self.will_send_shutdown_complete = false;

            let shutdown_complete = ChunkShutdownComplete {};
            ok = false;
            let p = self.create_packet(vec![Box::new(shutdown_complete)]);

            raw_packets.push(p);
        }

        (raw_packets, ok)
    }

    /// gather_outbound gathers outgoing packets. The returned bool value set to
    /// false means the association should be closed down after the final send.
    pub(crate) async fn gather_outbound(&mut self) -> (Vec<Packet>, bool) {
        let mut raw_packets = Vec::with_capacity(16);

        if !self.control_queue.is_empty() {
            for p in self.control_queue.drain(..) {
                raw_packets.push(p);
            }
        }

        let state = self.get_state();
        match state {
            AssociationState::Established => {
                raw_packets = self.gather_data_packets_to_retransmit(raw_packets);
                raw_packets = self
                    .gather_outbound_data_and_reconfig_packets(raw_packets)
                    .await;
                raw_packets = self.gather_outbound_fast_retransmission_packets(raw_packets);
                raw_packets = self.gather_outbound_sack_packets(raw_packets).await;
                raw_packets = self.gather_outbound_forward_tsn_packets(raw_packets);
                (raw_packets, true)
            }
            AssociationState::ShutdownPending
            | AssociationState::ShutdownSent
            | AssociationState::ShutdownReceived => {
                raw_packets = self.gather_data_packets_to_retransmit(raw_packets);
                raw_packets = self.gather_outbound_fast_retransmission_packets(raw_packets);
                raw_packets = self.gather_outbound_sack_packets(raw_packets).await;
                self.gather_outbound_shutdown_packets(raw_packets).await
            }
            AssociationState::ShutdownAckSent => {
                self.gather_outbound_shutdown_packets(raw_packets).await
            }
            _ => (raw_packets, true),
        }
    }

    /// set_state atomically sets the state of the Association.
    pub(crate) fn set_state(&self, new_state: AssociationState) {
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

    async fn handle_init(&mut self, p: &Packet, i: &ChunkInit) -> Result<Vec<Packet>> {
        let state = self.get_state();
        log::debug!("[{}] chunkInit received in state '{}'", self.name, state);

        // https://tools.ietf.org/html/rfc4960#section-5.2.1
        // Upon receipt of an INIT in the COOKIE-WAIT state, an endpoint MUST
        // respond with an INIT ACK using the same parameters it sent in its
        // original INIT chunk (including its Initiate Tag, unchanged).  When
        // responding, the endpoint MUST send the INIT ACK back to the same
        // address that the original INIT (sent by this endpoint) was sent.

        if state != AssociationState::Closed
            && state != AssociationState::CookieWait
            && state != AssociationState::CookieEchoed
        {
            // 5.2.2.  Unexpected INIT in States Other than CLOSED, COOKIE-ECHOED,
            //        COOKIE-WAIT, and SHUTDOWN-ACK-SENT
            return Err(Error::ErrHandleInitState);
        }

        // Should we be setting any of these permanently until we've ACKed further?
        self.my_max_num_inbound_streams =
            std::cmp::min(i.num_inbound_streams, self.my_max_num_inbound_streams);
        self.my_max_num_outbound_streams =
            std::cmp::min(i.num_outbound_streams, self.my_max_num_outbound_streams);
        self.peer_verification_tag = i.initiate_tag;
        self.source_port = p.destination_port;
        self.destination_port = p.source_port;

        // 13.2 This is the last TSN received in sequence.  This value
        // is set initially by taking the peer's initial TSN,
        // received in the INIT or INIT ACK chunk, and
        // subtracting one from it.
        self.peer_last_tsn = if i.initial_tsn == 0 {
            u32::MAX
        } else {
            i.initial_tsn - 1
        };

        for param in &i.params {
            if let Some(v) = param.as_any().downcast_ref::<ParamSupportedExtensions>() {
                for t in &v.chunk_types {
                    if *t == CT_FORWARD_TSN {
                        log::debug!("[{}] use ForwardTSN (on init)", self.name);
                        self.use_forward_tsn = true;
                    }
                }
            }
        }
        if !self.use_forward_tsn {
            log::warn!("[{}] not using ForwardTSN (on init)", self.name);
        }

        let mut outbound = Packet {
            verification_tag: self.peer_verification_tag,
            source_port: self.source_port,
            destination_port: self.destination_port,
            ..Default::default()
        };

        // According to RFC https://datatracker.ietf.org/doc/html/rfc4960#section-3.2.2
        // We report unknown parameters with a paramtype with bit 14 set as unrecognized
        let unrecognized_params_from_init = i
            .params
            .iter()
            .filter_map(|param| {
                if let ParamType::Unknown { param_type } = param.header().typ {
                    let needs_to_be_reported = ((param_type >> 14) & 0x01) == 1;
                    if needs_to_be_reported {
                        let wrapped: Box<dyn Param + Send + Sync> =
                            Box::new(ParamUnrecognized::wrap(param.clone()));
                        Some(wrapped)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        let mut init_ack = ChunkInit {
            is_ack: true,
            initial_tsn: self.my_next_tsn,
            num_outbound_streams: self.my_max_num_outbound_streams,
            num_inbound_streams: self.my_max_num_inbound_streams,
            initiate_tag: self.my_verification_tag,
            advertised_receiver_window_credit: self.max_receive_buffer_size,
            params: unrecognized_params_from_init,
        };

        if self.my_cookie.is_none() {
            self.my_cookie = Some(ParamStateCookie::new());
        }

        if let Some(my_cookie) = &self.my_cookie {
            init_ack.params = vec![Box::new(my_cookie.clone())];
        }

        init_ack.set_supported_extensions();

        outbound.chunks = vec![Box::new(init_ack)];

        Ok(vec![outbound])
    }

    async fn handle_init_ack(&mut self, p: &Packet, i: &ChunkInit) -> Result<Vec<Packet>> {
        let state = self.get_state();
        log::debug!("[{}] chunkInitAck received in state '{}'", self.name, state);
        if state != AssociationState::CookieWait {
            // RFC 4960
            // 5.2.3.  Unexpected INIT ACK
            //   If an INIT ACK is received by an endpoint in any state other than the
            //   COOKIE-WAIT state, the endpoint should discard the INIT ACK chunk.
            //   An unexpected INIT ACK usually indicates the processing of an old or
            //   duplicated INIT chunk.
            return Ok(vec![]);
        }

        self.my_max_num_inbound_streams =
            std::cmp::min(i.num_inbound_streams, self.my_max_num_inbound_streams);
        self.my_max_num_outbound_streams =
            std::cmp::min(i.num_outbound_streams, self.my_max_num_outbound_streams);
        self.peer_verification_tag = i.initiate_tag;
        self.peer_last_tsn = if i.initial_tsn == 0 {
            u32::MAX
        } else {
            i.initial_tsn - 1
        };
        if self.source_port != p.destination_port || self.destination_port != p.source_port {
            log::warn!("[{}] handle_init_ack: port mismatch", self.name);
            return Ok(vec![]);
        }

        self.rwnd = i.advertised_receiver_window_credit;
        log::debug!("[{}] initial rwnd={}", self.name, self.rwnd);

        // RFC 4690 Sec 7.2.1
        //  o  The initial value of ssthresh MAY be arbitrarily high (for
        //     example, implementations MAY use the size of the receiver
        //     advertised window).
        self.ssthresh = self.rwnd;
        log::trace!(
            "[{}] updated cwnd={} ssthresh={} inflight={} (INI)",
            self.name,
            self.cwnd,
            self.ssthresh,
            self.inflight_queue.get_num_bytes()
        );

        if let Some(t1init) = &self.t1init {
            t1init.stop().await;
        }
        self.stored_init = None;

        let mut cookie_param = None;
        for param in &i.params {
            if let Some(v) = param.as_any().downcast_ref::<ParamStateCookie>() {
                cookie_param = Some(v);
            } else if let Some(v) = param.as_any().downcast_ref::<ParamSupportedExtensions>() {
                for t in &v.chunk_types {
                    if *t == CT_FORWARD_TSN {
                        log::debug!("[{}] use ForwardTSN (on initAck)", self.name);
                        self.use_forward_tsn = true;
                    }
                }
            }
        }
        if !self.use_forward_tsn {
            log::warn!("[{}] not using ForwardTSN (on initAck)", self.name);
        }

        if let Some(v) = cookie_param {
            self.stored_cookie_echo = Some(ChunkCookieEcho {
                cookie: v.cookie.clone(),
            });

            self.send_cookie_echo()?;

            if let Some(t1cookie) = &self.t1cookie {
                t1cookie.start(self.rto_mgr.get_rto()).await;
            }

            self.set_state(AssociationState::CookieEchoed);

            Ok(vec![])
        } else {
            Err(Error::ErrInitAckNoCookie)
        }
    }

    async fn handle_heartbeat(&self, c: &ChunkHeartbeat) -> Result<Vec<Packet>> {
        log::trace!("[{}] chunkHeartbeat", self.name);
        if let Some(p) = c.params.first() {
            if let Some(hbi) = p.as_any().downcast_ref::<ParamHeartbeatInfo>() {
                return Ok(vec![Packet {
                    verification_tag: self.peer_verification_tag,
                    source_port: self.source_port,
                    destination_port: self.destination_port,
                    chunks: vec![Box::new(ChunkHeartbeatAck {
                        params: vec![Box::new(ParamHeartbeatInfo {
                            heartbeat_information: hbi.heartbeat_information.clone(),
                        })],
                    })],
                }]);
            } else {
                log::warn!(
                    "[{}] failed to handle Heartbeat, no ParamHeartbeatInfo",
                    self.name,
                );
            }
        }

        Ok(vec![])
    }

    async fn handle_cookie_echo(&mut self, c: &ChunkCookieEcho) -> Result<Vec<Packet>> {
        let state = self.get_state();
        log::debug!("[{}] COOKIE-ECHO received in state '{}'", self.name, state);

        if let Some(my_cookie) = &self.my_cookie {
            match state {
                AssociationState::Established => {
                    if my_cookie.cookie != c.cookie {
                        return Ok(vec![]);
                    }
                }
                AssociationState::Closed
                | AssociationState::CookieWait
                | AssociationState::CookieEchoed => {
                    if my_cookie.cookie != c.cookie {
                        return Ok(vec![]);
                    }

                    if let Some(t1init) = &self.t1init {
                        t1init.stop().await;
                    }
                    self.stored_init = None;

                    if let Some(t1cookie) = &self.t1cookie {
                        t1cookie.stop().await;
                    }
                    self.stored_cookie_echo = None;

                    self.set_state(AssociationState::Established);
                    if let Some(handshake_completed_ch) = &self.handshake_completed_ch_tx {
                        let _ = handshake_completed_ch.send(None).await;
                    }
                }
                _ => return Ok(vec![]),
            };
        } else {
            log::debug!("[{}] COOKIE-ECHO received before initialization", self.name);
            return Ok(vec![]);
        }

        Ok(vec![Packet {
            verification_tag: self.peer_verification_tag,
            source_port: self.source_port,
            destination_port: self.destination_port,
            chunks: vec![Box::new(ChunkCookieAck {})],
        }])
    }

    async fn handle_cookie_ack(&mut self) -> Result<Vec<Packet>> {
        let state = self.get_state();
        log::debug!("[{}] COOKIE-ACK received in state '{}'", self.name, state);
        if state != AssociationState::CookieEchoed {
            // RFC 4960
            // 5.2.5.  Handle Duplicate COOKIE-ACK.
            //   At any state other than COOKIE-ECHOED, an endpoint should silently
            //   discard a received COOKIE ACK chunk.
            return Ok(vec![]);
        }

        if let Some(t1cookie) = &self.t1cookie {
            t1cookie.stop().await;
        }
        self.stored_cookie_echo = None;

        self.set_state(AssociationState::Established);
        if let Some(handshake_completed_ch) = &self.handshake_completed_ch_tx {
            let _ = handshake_completed_ch.send(None).await;
        }

        Ok(vec![])
    }

    async fn handle_data(&mut self, d: &ChunkPayloadData) -> Result<Vec<Packet>> {
        log::trace!(
            "[{}] DATA: tsn={} immediateSack={} len={}",
            self.name,
            d.tsn,
            d.immediate_sack,
            d.user_data.len()
        );
        self.stats.inc_datas();

        let can_push = self.payload_queue.can_push(d, self.peer_last_tsn);
        let mut stream_handle_data = false;
        if can_push {
            if let Some(_s) = self.get_or_create_stream(d.stream_identifier) {
                if self.get_my_receiver_window_credit().await > 0 {
                    // Pass the new chunk to stream level as soon as it arrives
                    self.payload_queue.push(d.clone(), self.peer_last_tsn);
                    stream_handle_data = true;
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
                return Ok(vec![]);
            }
        }

        let immediate_sack = d.immediate_sack;

        if stream_handle_data {
            if let Some(s) = self.streams.get_mut(&d.stream_identifier) {
                s.handle_data(d.clone()).await;
            }
        }

        self.handle_peer_last_tsn_and_acknowledgement(immediate_sack)
    }

    /// A common routine for handle_data and handle_forward_tsn routines
    fn handle_peer_last_tsn_and_acknowledgement(
        &mut self,
        sack_immediately: bool,
    ) -> Result<Vec<Packet>> {
        let mut reply = vec![];

        // Try to advance peer_last_tsn

        // From RFC 3758 Sec 3.6:
        //   .. and then MUST further advance its cumulative TSN point locally
        //   if possible
        // Meaning, if peer_last_tsn+1 points to a chunk that is received,
        // advance peer_last_tsn until peer_last_tsn+1 points to unreceived chunk.
        log::debug!("[{}] peer_last_tsn = {}", self.name, self.peer_last_tsn);
        while self.payload_queue.pop(self.peer_last_tsn + 1).is_some() {
            self.peer_last_tsn += 1;
            log::debug!("[{}] peer_last_tsn = {}", self.name, self.peer_last_tsn);

            let rst_reqs: Vec<ParamOutgoingResetRequest> =
                self.reconfig_requests.values().cloned().collect();
            for rst_req in rst_reqs {
                let resp = self.reset_streams_if_any(&rst_req);
                log::debug!("[{}] RESET RESPONSE: {}", self.name, resp);
                reply.push(resp);
            }
        }

        let has_packet_loss = !self.payload_queue.is_empty();
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

        Ok(reply)
    }

    pub(crate) async fn get_my_receiver_window_credit(&self) -> u32 {
        let mut bytes_queued = 0;
        for s in self.streams.values() {
            bytes_queued += s.get_num_bytes_in_reassembly_queue().await as u32;
        }

        if bytes_queued >= self.max_receive_buffer_size {
            0
        } else {
            self.max_receive_buffer_size - bytes_queued
        }
    }

    pub(crate) fn open_stream(
        &mut self,
        stream_identifier: u16,
        default_payload_type: PayloadProtocolIdentifier,
    ) -> Result<Arc<Stream>> {
        if self.streams.contains_key(&stream_identifier) {
            return Err(Error::ErrStreamAlreadyExist);
        }

        if let Some(s) = self.create_stream(stream_identifier, false) {
            s.set_default_payload_type(default_payload_type);
            Ok(Arc::clone(&s))
        } else {
            Err(Error::ErrStreamCreateFailed)
        }
    }

    /// create_stream creates a stream. The caller should hold the lock and check no stream exists for this id.
    fn create_stream(&mut self, stream_identifier: u16, accept: bool) -> Option<Arc<Stream>> {
        let s = Arc::new(Stream::new(
            format!("{}:{}", stream_identifier, self.name),
            stream_identifier,
            self.max_payload_size,
            Arc::clone(&self.max_message_size),
            Arc::clone(&self.state),
            self.awake_write_loop_ch.clone(),
            Arc::clone(&self.pending_queue),
        ));

        if accept {
            if let Some(accept_ch) = &self.accept_ch_tx {
                if accept_ch.try_send(Arc::clone(&s)).is_ok() {
                    log::debug!(
                        "[{}] accepted a new stream (streamIdentifier: {})",
                        self.name,
                        stream_identifier
                    );
                } else {
                    log::debug!("[{}] dropped a new stream due to accept_ch full", self.name);
                    return None;
                }
            } else {
                log::debug!(
                    "[{}] dropped a new stream due to accept_ch_tx is None",
                    self.name
                );
                return None;
            }
        }
        self.streams.insert(stream_identifier, Arc::clone(&s));
        Some(s)
    }

    /// get_or_create_stream gets or creates a stream. The caller should hold the lock.
    fn get_or_create_stream(&mut self, stream_identifier: u16) -> Option<Arc<Stream>> {
        if self.streams.contains_key(&stream_identifier) {
            self.streams.get(&stream_identifier).cloned()
        } else {
            self.create_stream(stream_identifier, true)
        }
    }

    async fn process_selective_ack(
        &mut self,
        d: &ChunkSelectiveAck,
    ) -> Result<(HashMap<u16, i64>, u32)> {
        let mut bytes_acked_per_stream = HashMap::new();

        // New ack point, so pop all ACKed packets from inflight_queue
        // We add 1 because the "currentAckPoint" has already been popped from the inflight queue
        // For the first SACK we take care of this by setting the ackpoint to cumAck - 1
        let mut i = self.cumulative_tsn_ack_point + 1;
        //log::debug!("[{}] i={} d={}", self.name, i, d.cumulative_tsn_ack);
        while sna32lte(i, d.cumulative_tsn_ack) {
            if let Some(c) = self.inflight_queue.pop(i) {
                if !c.acked {
                    // RFC 4096 sec 6.3.2.  Retransmission Timer Rules
                    //   R3)  Whenever a SACK is received that acknowledges the DATA chunk
                    //        with the earliest outstanding TSN for that address, restart the
                    //        T3-rtx timer for that address with its current RTO (if there is
                    //        still outstanding data on that address).
                    if i == self.cumulative_tsn_ack_point + 1 {
                        // T3 timer needs to be reset. Stop it for now.
                        if let Some(t3rtx) = &self.t3rtx {
                            t3rtx.stop().await;
                        }
                    }

                    let n_bytes_acked = c.user_data.len() as i64;

                    // Sum the number of bytes acknowledged per stream
                    if let Some(amount) = bytes_acked_per_stream.get_mut(&c.stream_identifier) {
                        *amount += n_bytes_acked;
                    } else {
                        bytes_acked_per_stream.insert(c.stream_identifier, n_bytes_acked);
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
                    if c.nsent == 1 && sna32gte(c.tsn, self.min_tsn2measure_rtt) {
                        self.min_tsn2measure_rtt = self.my_next_tsn;
                        let rtt = match SystemTime::now().duration_since(c.since) {
                            Ok(rtt) => rtt,
                            Err(_) => return Err(Error::ErrInvalidSystemTime),
                        };
                        let srtt = self.rto_mgr.set_new_rtt(rtt.as_millis() as u64);
                        log::trace!(
                            "[{}] SACK: measured-rtt={} srtt={} new-rto={}",
                            self.name,
                            rtt.as_millis(),
                            srtt,
                            self.rto_mgr.get_rto()
                        );
                    }
                }

                if self.in_fast_recovery && c.tsn == self.fast_recover_exit_point {
                    log::debug!("[{}] exit fast-recovery", self.name);
                    self.in_fast_recovery = false;
                }
            } else {
                return Err(Error::ErrInflightQueueTsnPop);
            }

            i += 1;
        }

        let mut htna = d.cumulative_tsn_ack;

        // Mark selectively acknowledged chunks as "acked"
        for g in &d.gap_ack_blocks {
            for i in g.start..=g.end {
                let tsn = d.cumulative_tsn_ack + i as u32;

                let (is_existed, is_acked) = if let Some(c) = self.inflight_queue.get(tsn) {
                    (true, c.acked)
                } else {
                    (false, false)
                };
                let n_bytes_acked = if is_existed && !is_acked {
                    self.inflight_queue.mark_as_acked(tsn) as i64
                } else {
                    0
                };

                if let Some(c) = self.inflight_queue.get(tsn) {
                    if !is_acked {
                        // Sum the number of bytes acknowledged per stream
                        if let Some(amount) = bytes_acked_per_stream.get_mut(&c.stream_identifier) {
                            *amount += n_bytes_acked;
                        } else {
                            bytes_acked_per_stream.insert(c.stream_identifier, n_bytes_acked);
                        }

                        log::trace!("[{}] tsn={} has been sacked", self.name, c.tsn);

                        if c.nsent == 1 {
                            self.min_tsn2measure_rtt = self.my_next_tsn;
                            let rtt = match SystemTime::now().duration_since(c.since) {
                                Ok(rtt) => rtt,
                                Err(_) => return Err(Error::ErrInvalidSystemTime),
                            };
                            let srtt = self.rto_mgr.set_new_rtt(rtt.as_millis() as u64);
                            log::trace!(
                                "[{}] SACK: measured-rtt={} srtt={} new-rto={}",
                                self.name,
                                rtt.as_millis(),
                                srtt,
                                self.rto_mgr.get_rto()
                            );
                        }

                        if sna32lt(htna, tsn) {
                            htna = tsn;
                        }
                    }
                } else {
                    return Err(Error::ErrTsnRequestNotExist);
                }
            }
        }

        Ok((bytes_acked_per_stream, htna))
    }

    async fn on_cumulative_tsn_ack_point_advanced(&mut self, total_bytes_acked: i64) {
        // RFC 4096, sec 6.3.2.  Retransmission Timer Rules
        //   R2)  Whenever all outstanding data sent to an address have been
        //        acknowledged, turn off the T3-rtx timer of that address.
        if self.inflight_queue.is_empty() {
            log::trace!(
                "[{}] SACK: no more packet in-flight (pending={})",
                self.name,
                self.pending_queue.len()
            );
            if let Some(t3rtx) = &self.t3rtx {
                t3rtx.stop().await;
            }
        } else {
            log::trace!("[{}] T3-rtx timer start (pt2)", self.name);
            if let Some(t3rtx) = &self.t3rtx {
                t3rtx.start(self.rto_mgr.get_rto()).await;
            }
        }

        // Update congestion control parameters
        if self.cwnd <= self.ssthresh {
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
            if !self.in_fast_recovery && self.pending_queue.len() > 0 {
                self.cwnd += std::cmp::min(total_bytes_acked as u32, self.cwnd); // TCP way
                                                                                 // self.cwnd += min32(uint32(total_bytes_acked), self.mtu) // SCTP way (slow)
                log::trace!(
                    "[{}] updated cwnd={} ssthresh={} acked={} (SS)",
                    self.name,
                    self.cwnd,
                    self.ssthresh,
                    total_bytes_acked
                );
            } else {
                log::trace!(
                    "[{}] cwnd did not grow: cwnd={} ssthresh={} acked={} FR={} pending={}",
                    self.name,
                    self.cwnd,
                    self.ssthresh,
                    total_bytes_acked,
                    self.in_fast_recovery,
                    self.pending_queue.len()
                );
            }
        } else {
            // RFC 4096, sec 7.2.2.  Congestion Avoidance
            //   o  Whenever cwnd is greater than ssthresh, upon each SACK arrival
            //      that advances the Cumulative TSN Ack Point, increase
            //      partial_bytes_acked by the total number of bytes of all new chunks
            //      acknowledged in that SACK including chunks acknowledged by the new
            //      Cumulative TSN Ack and by Gap Ack Blocks.
            self.partial_bytes_acked += total_bytes_acked as u32;

            //   o  When partial_bytes_acked is equal to or greater than cwnd and
            //      before the arrival of the SACK the sender had cwnd or more bytes
            //      of data outstanding (i.e., before arrival of the SACK, flight size
            //      was greater than or equal to cwnd), increase cwnd by MTU, and
            //      reset partial_bytes_acked to (partial_bytes_acked - cwnd).
            if self.partial_bytes_acked >= self.cwnd && self.pending_queue.len() > 0 {
                self.partial_bytes_acked -= self.cwnd;
                self.cwnd += self.mtu;
                log::trace!(
                    "[{}] updated cwnd={} ssthresh={} acked={} (CA)",
                    self.name,
                    self.cwnd,
                    self.ssthresh,
                    total_bytes_acked
                );
            }
        }
    }

    fn process_fast_retransmission(
        &mut self,
        cum_tsn_ack_point: u32,
        htna: u32,
        cum_tsn_ack_point_advanced: bool,
    ) -> Result<()> {
        // HTNA algorithm - RFC 4960 Sec 7.2.4
        // Increment missIndicator of each chunks that the SACK reported missing
        // when either of the following is met:
        // a)  Not in fast-recovery
        //     miss indications are incremented only for missing TSNs prior to the
        //     highest TSN newly acknowledged in the SACK.
        // b)  In fast-recovery AND the Cumulative TSN Ack Point advanced
        //     the miss indications are incremented for all TSNs reported missing
        //     in the SACK.
        if !self.in_fast_recovery || cum_tsn_ack_point_advanced {
            let max_tsn = if !self.in_fast_recovery {
                // a) increment only for missing TSNs prior to the HTNA
                htna
            } else {
                // b) increment for all TSNs reported missing
                cum_tsn_ack_point + (self.inflight_queue.len() as u32) + 1
            };

            let mut tsn = cum_tsn_ack_point + 1;
            while sna32lt(tsn, max_tsn) {
                if let Some(c) = self.inflight_queue.get_mut(tsn) {
                    if !c.acked && !c.abandoned() && c.miss_indicator < 3 {
                        c.miss_indicator += 1;
                        if c.miss_indicator == 3 && !self.in_fast_recovery {
                            // 2)  If not in Fast Recovery, adjust the ssthresh and cwnd of the
                            //     destination address(es) to which the missing DATA chunks were
                            //     last sent, according to the formula described in Section 7.2.3.
                            self.in_fast_recovery = true;
                            self.fast_recover_exit_point = htna;
                            self.ssthresh = std::cmp::max(self.cwnd / 2, 4 * self.mtu);
                            self.cwnd = self.ssthresh;
                            self.partial_bytes_acked = 0;
                            self.will_retransmit_fast = true;

                            log::trace!(
                                "[{}] updated cwnd={} ssthresh={} inflight={} (FR)",
                                self.name,
                                self.cwnd,
                                self.ssthresh,
                                self.inflight_queue.get_num_bytes()
                            );
                        }
                    }
                } else {
                    return Err(Error::ErrTsnRequestNotExist);
                }

                tsn += 1;
            }
        }

        if self.in_fast_recovery && cum_tsn_ack_point_advanced {
            self.will_retransmit_fast = true;
        }

        Ok(())
    }

    async fn handle_sack(&mut self, d: &ChunkSelectiveAck) -> Result<Vec<Packet>> {
        log::trace!(
            "[{}] {}, SACK: cumTSN={} a_rwnd={}",
            self.name,
            self.cumulative_tsn_ack_point,
            d.cumulative_tsn_ack,
            d.advertised_receiver_window_credit
        );
        let state = self.get_state();
        if state != AssociationState::Established
            && state != AssociationState::ShutdownPending
            && state != AssociationState::ShutdownReceived
        {
            return Ok(vec![]);
        }

        self.stats.inc_sacks();

        if sna32gt(self.cumulative_tsn_ack_point, d.cumulative_tsn_ack) {
            // RFC 4960 sec 6.2.1.  Processing a Received SACK
            // D)
            //   i) If Cumulative TSN Ack is less than the Cumulative TSN Ack
            //      Point, then drop the SACK.  Since Cumulative TSN Ack is
            //      monotonically increasing, a SACK whose Cumulative TSN Ack is
            //      less than the Cumulative TSN Ack Point indicates an out-of-
            //      order SACK.

            log::debug!(
                "[{}] SACK Cumulative ACK {} is older than ACK point {}",
                self.name,
                d.cumulative_tsn_ack,
                self.cumulative_tsn_ack_point
            );

            return Ok(vec![]);
        }

        // Process selective ack
        let (bytes_acked_per_stream, htna) = self.process_selective_ack(d).await?;

        let mut total_bytes_acked = 0;
        for n_bytes_acked in bytes_acked_per_stream.values() {
            total_bytes_acked += *n_bytes_acked;
        }

        let mut cum_tsn_ack_point_advanced = false;
        if sna32lt(self.cumulative_tsn_ack_point, d.cumulative_tsn_ack) {
            log::trace!(
                "[{}] SACK: cumTSN advanced: {} -> {}",
                self.name,
                self.cumulative_tsn_ack_point,
                d.cumulative_tsn_ack
            );

            self.cumulative_tsn_ack_point = d.cumulative_tsn_ack;
            cum_tsn_ack_point_advanced = true;
            self.on_cumulative_tsn_ack_point_advanced(total_bytes_acked)
                .await;
        }

        for (si, n_bytes_acked) in &bytes_acked_per_stream {
            if let Some(s) = self.streams.get_mut(si) {
                s.on_buffer_released(*n_bytes_acked).await;
            }
        }

        // New rwnd value
        // RFC 4960 sec 6.2.1.  Processing a Received SACK
        // D)
        //   ii) Set rwnd equal to the newly received a_rwnd minus the number
        //       of bytes still outstanding after processing the Cumulative
        //       TSN Ack and the Gap Ack Blocks.

        // bytes acked were already subtracted by markAsAcked() method
        let bytes_outstanding = self.inflight_queue.get_num_bytes() as u32;
        if bytes_outstanding >= d.advertised_receiver_window_credit {
            self.rwnd = 0;
        } else {
            self.rwnd = d.advertised_receiver_window_credit - bytes_outstanding;
        }

        self.process_fast_retransmission(d.cumulative_tsn_ack, htna, cum_tsn_ack_point_advanced)?;

        if self.use_forward_tsn {
            // RFC 3758 Sec 3.5 C1
            if sna32lt(
                self.advanced_peer_tsn_ack_point,
                self.cumulative_tsn_ack_point,
            ) {
                self.advanced_peer_tsn_ack_point = self.cumulative_tsn_ack_point
            }

            // RFC 3758 Sec 3.5 C2
            let mut i = self.advanced_peer_tsn_ack_point + 1;
            while let Some(c) = self.inflight_queue.get(i) {
                if !c.abandoned() {
                    break;
                }
                self.advanced_peer_tsn_ack_point = i;
                i += 1;
            }

            // RFC 3758 Sec 3.5 C3
            if sna32gt(
                self.advanced_peer_tsn_ack_point,
                self.cumulative_tsn_ack_point,
            ) {
                self.will_send_forward_tsn = true;
                log::debug!(
                    "[{}] handleSack {}: sna32GT({}, {})",
                    self.name,
                    self.will_send_forward_tsn,
                    self.advanced_peer_tsn_ack_point,
                    self.cumulative_tsn_ack_point
                );
            }
            self.awake_write_loop();
        }

        self.postprocess_sack(state, cum_tsn_ack_point_advanced)
            .await;

        Ok(vec![])
    }

    /// The caller must hold the lock. This method was only added because the
    /// linter was complaining about the "cognitive complexity" of handle_sack.
    async fn postprocess_sack(
        &mut self,
        state: AssociationState,
        mut should_awake_write_loop: bool,
    ) {
        if !self.inflight_queue.is_empty() {
            // Start timer. (noop if already started)
            log::trace!("[{}] T3-rtx timer start (pt3)", self.name);
            if let Some(t3rtx) = &self.t3rtx {
                t3rtx.start(self.rto_mgr.get_rto()).await;
            }
        } else if state == AssociationState::ShutdownPending {
            // No more outstanding, send shutdown.
            should_awake_write_loop = true;
            self.will_send_shutdown.store(true, Ordering::SeqCst);
            self.set_state(AssociationState::ShutdownSent);
        } else if state == AssociationState::ShutdownReceived {
            // No more outstanding, send shutdown ack.
            should_awake_write_loop = true;
            self.will_send_shutdown_ack = true;
            self.set_state(AssociationState::ShutdownAckSent);
        }

        if should_awake_write_loop {
            self.awake_write_loop();
        }
    }

    async fn handle_shutdown(&mut self, _: &ChunkShutdown) -> Result<Vec<Packet>> {
        let state = self.get_state();

        if state == AssociationState::Established {
            if !self.inflight_queue.is_empty() {
                self.set_state(AssociationState::ShutdownReceived);
            } else {
                // No more outstanding, send shutdown ack.
                self.will_send_shutdown_ack = true;
                self.set_state(AssociationState::ShutdownAckSent);

                self.awake_write_loop();
            }
        } else if state == AssociationState::ShutdownSent {
            // self.cumulative_tsn_ack_point = c.cumulative_tsn_ack

            self.will_send_shutdown_ack = true;
            self.set_state(AssociationState::ShutdownAckSent);

            self.awake_write_loop();
        }

        Ok(vec![])
    }

    async fn handle_shutdown_ack(&mut self, _: &ChunkShutdownAck) -> Result<Vec<Packet>> {
        let state = self.get_state();
        if state == AssociationState::ShutdownSent || state == AssociationState::ShutdownAckSent {
            if let Some(t2shutdown) = &self.t2shutdown {
                t2shutdown.stop().await;
            }
            self.will_send_shutdown_complete = true;

            self.awake_write_loop();
        }

        Ok(vec![])
    }

    async fn handle_shutdown_complete(&mut self, _: &ChunkShutdownComplete) -> Result<Vec<Packet>> {
        let state = self.get_state();
        if state == AssociationState::ShutdownAckSent {
            if let Some(t2shutdown) = &self.t2shutdown {
                t2shutdown.stop().await;
            }
            self.close().await?;
        }

        Ok(vec![])
    }

    /// create_forward_tsn generates ForwardTSN chunk.
    /// This method will be be called if use_forward_tsn is set to false.
    fn create_forward_tsn(&self) -> ChunkForwardTsn {
        // RFC 3758 Sec 3.5 C4
        let mut stream_map: HashMap<u16, u16> = HashMap::new(); // to report only once per SI
        let mut i = self.cumulative_tsn_ack_point + 1;
        while sna32lte(i, self.advanced_peer_tsn_ack_point) {
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
            new_cumulative_tsn: self.advanced_peer_tsn_ack_point,
            streams: vec![],
        };

        let mut stream_str = String::new();
        for (si, ssn) in &stream_map {
            stream_str += format!("(si={si} ssn={ssn})").as_str();
            fwd_tsn.streams.push(ChunkForwardTsnStream {
                identifier: *si,
                sequence: *ssn,
            });
        }
        log::trace!(
            "[{}] building fwd_tsn: newCumulativeTSN={} cumTSN={} - {}",
            self.name,
            fwd_tsn.new_cumulative_tsn,
            self.cumulative_tsn_ack_point,
            stream_str
        );

        fwd_tsn
    }

    /// create_packet wraps chunks in a packet.
    /// The caller should hold the read lock.
    pub(crate) fn create_packet(&self, chunks: Vec<Box<dyn Chunk + Send + Sync>>) -> Packet {
        Packet {
            verification_tag: self.peer_verification_tag,
            source_port: self.source_port,
            destination_port: self.destination_port,
            chunks,
        }
    }

    async fn handle_reconfig(&mut self, c: &ChunkReconfig) -> Result<Vec<Packet>> {
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

    async fn handle_forward_tsn(&mut self, c: &ChunkForwardTsn) -> Result<Vec<Packet>> {
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
            return Ok(vec![outbound]);
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
            if let Some(ack_timer) = &mut self.ack_timer {
                ack_timer.stop();
            }
            self.awake_write_loop();
            return Ok(vec![]);
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
                s.handle_forward_tsn_for_ordered(forwarded.sequence).await;
            }
        }

        // TSN may be forewared for unordered chunks. ForwardTSN chunk does not
        // report which stream identifier it skipped for unordered chunks.
        // Therefore, we need to broadcast this event to all existing streams for
        // unordered chunks.
        // See https://github.com/pion/sctp/issues/106
        for s in self.streams.values_mut() {
            s.handle_forward_tsn_for_unordered(c.new_cumulative_tsn)
                .await;
        }

        self.handle_peer_last_tsn_and_acknowledgement(false)
    }

    async fn send_reset_request(&mut self, stream_identifier: u16) -> Result<()> {
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

        self.pending_queue.push(c).await;
        self.awake_write_loop();

        Ok(())
    }

    #[allow(clippy::borrowed_box)]
    async fn handle_reconfig_param(
        &mut self,
        raw: &Box<dyn Param + Send + Sync>,
    ) -> Result<Option<Packet>> {
        if let Some(p) = raw.as_any().downcast_ref::<ParamOutgoingResetRequest>() {
            self.reconfig_requests
                .insert(p.reconfig_request_sequence_number, p.clone());
            Ok(Some(self.reset_streams_if_any(p)))
        } else if let Some(p) = raw.as_any().downcast_ref::<ParamReconfigResponse>() {
            self.reconfigs.remove(&p.reconfig_response_sequence_number);
            if self.reconfigs.is_empty() {
                if let Some(treconfig) = &self.treconfig {
                    treconfig.stop().await;
                }
            }
            Ok(None)
        } else {
            Err(Error::ErrParamterType)
        }
    }

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
                    self.unregister_stream(stream_identifier);
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

    /// Move the chunk peeked with self.pending_queue.peek() to the inflight_queue.
    async fn move_pending_data_chunk_to_inflight_queue(
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
                c.payload_type as u32,
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
    async fn pop_pending_data_chunks_to_send(&mut self) -> (Vec<ChunkPayloadData>, Vec<u16>) {
        let mut chunks = vec![];
        let mut sis_to_reset = vec![]; // stream identifiers to reset

        if self.pending_queue.len() == 0 {
            return (chunks, sis_to_reset);
        }

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
                break; // would exceed cwnd
            }

            if data_len > self.rwnd as usize {
                break; // no more rwnd
            }

            self.rwnd -= data_len as u32;

            if let Some(chunk) = self
                .move_pending_data_chunk_to_inflight_queue(beginning_fragment, unordered)
                .await
            {
                chunks.push(chunk);
            }
        }

        // the data sender can always have one DATA chunk in flight to the receiver
        if chunks.is_empty() && self.inflight_queue.is_empty() {
            // Send zero window probe
            if let Some(c) = self.pending_queue.peek() {
                let (beginning_fragment, unordered) = (c.beginning_fragment, c.unordered);

                if let Some(chunk) = self
                    .move_pending_data_chunk_to_inflight_queue(beginning_fragment, unordered)
                    .await
                {
                    chunks.push(chunk);
                }
            }
        }

        (chunks, sis_to_reset)
    }

    /// bundle_data_chunks_into_packets packs DATA chunks into packets. It tries to bundle
    /// DATA chunks into a packet so long as the resulting packet size does not exceed
    /// the path MTU.
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
            let reliability_type: ReliabilityType =
                s.reliability_type.load(Ordering::SeqCst).into();
            let reliability_value = s.reliability_value.load(Ordering::SeqCst);

            if reliability_type == ReliabilityType::Rexmit {
                if c.nsent >= reliability_value {
                    c.set_abandoned(true);
                    log::trace!(
                        "[{}] marked as abandoned: tsn={} ppi={} (remix: {})",
                        self.name,
                        c.tsn,
                        c.payload_type,
                        c.nsent
                    );
                }
            } else if reliability_type == ReliabilityType::Timed {
                if let Ok(elapsed) = SystemTime::now().duration_since(c.since) {
                    if elapsed.as_millis() as u32 >= reliability_value {
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
    fn get_data_packets_to_retransmit(&mut self) -> Vec<Packet> {
        let awnd = std::cmp::min(self.cwnd, self.rwnd);
        let mut chunks = vec![];
        let mut bytes_to_send = 0;
        let mut done = false;
        let mut i = 0;
        while !done {
            let tsn = self.cumulative_tsn_ack_point + i + 1;
            if let Some(c) = self.inflight_queue.get_mut(tsn) {
                if !c.retransmit {
                    i += 1;
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
    fn generate_next_tsn(&mut self) -> u32 {
        let tsn = self.my_next_tsn;
        self.my_next_tsn += 1;
        tsn
    }

    /// generate_next_rsn returns the my_next_rsn and increases it. The caller should hold the lock.
    fn generate_next_rsn(&mut self) -> u32 {
        let rsn = self.my_next_rsn;
        self.my_next_rsn += 1;
        rsn
    }

    async fn create_selective_ack_chunk(&mut self) -> ChunkSelectiveAck {
        ChunkSelectiveAck {
            cumulative_tsn_ack: self.peer_last_tsn,
            advertised_receiver_window_credit: self.get_my_receiver_window_credit().await,
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

    fn handle_chunk_end(&mut self) {
        if self.immediate_ack_triggered {
            self.ack_state = AckState::Immediate;
            if let Some(ack_timer) = &mut self.ack_timer {
                ack_timer.stop();
            }
            self.awake_write_loop();
        } else if self.delayed_ack_triggered {
            // Will send delayed ack in the next ack timeout
            self.ack_state = AckState::Delay;
            if let Some(ack_timer) = &mut self.ack_timer {
                ack_timer.start();
            }
        }
    }

    #[allow(clippy::borrowed_box)]
    async fn handle_chunk(
        &mut self,
        p: &Packet,
        chunk: &Box<dyn Chunk + Send + Sync>,
    ) -> Result<()> {
        chunk.check()?;
        let chunk_any = chunk.as_any();
        let packets = if let Some(c) = chunk_any.downcast_ref::<ChunkInit>() {
            if c.is_ack {
                self.handle_init_ack(p, c).await?
            } else {
                self.handle_init(p, c).await?
            }
        } else if chunk_any.downcast_ref::<ChunkAbort>().is_some()
            || chunk_any.downcast_ref::<ChunkError>().is_some()
        {
            return Err(Error::ErrChunk);
        } else if let Some(c) = chunk_any.downcast_ref::<ChunkHeartbeat>() {
            self.handle_heartbeat(c).await?
        } else if let Some(c) = chunk_any.downcast_ref::<ChunkCookieEcho>() {
            self.handle_cookie_echo(c).await?
        } else if chunk_any.downcast_ref::<ChunkCookieAck>().is_some() {
            self.handle_cookie_ack().await?
        } else if let Some(c) = chunk_any.downcast_ref::<ChunkPayloadData>() {
            self.handle_data(c).await?
        } else if let Some(c) = chunk_any.downcast_ref::<ChunkSelectiveAck>() {
            self.handle_sack(c).await?
        } else if let Some(c) = chunk_any.downcast_ref::<ChunkReconfig>() {
            self.handle_reconfig(c).await?
        } else if let Some(c) = chunk_any.downcast_ref::<ChunkForwardTsn>() {
            self.handle_forward_tsn(c).await?
        } else if let Some(c) = chunk_any.downcast_ref::<ChunkShutdown>() {
            self.handle_shutdown(c).await?
        } else if let Some(c) = chunk_any.downcast_ref::<ChunkShutdownAck>() {
            self.handle_shutdown_ack(c).await?
        } else if let Some(c) = chunk_any.downcast_ref::<ChunkShutdownComplete>() {
            self.handle_shutdown_complete(c).await?
        } else {
            /*
            https://datatracker.ietf.org/doc/html/rfc4960#section-3

            00 -  Stop processing this SCTP packet and discard it, do not
                  process any further chunks within it.

            01 -  Stop processing this SCTP packet and discard it, do not
                  process any further chunks within it, and report the
                  unrecognized chunk in an 'Unrecognized Chunk Type'.

            10 -  Skip this chunk and continue processing.

            11 -  Skip this chunk and continue processing, but report in an
                  ERROR chunk using the 'Unrecognized Chunk Type' cause of
                  error.
            */
            let handle_code = chunk.header().typ.0 >> 6;
            match handle_code {
                0b00 => {
                    // Stop processing this packet
                    return Err(Error::ErrChunkTypeUnhandled);
                }
                0b01 => {
                    // stop processing but report the chunk as unrecognized
                    let err_chunk = ChunkError {
                        error_causes: vec![ErrorCause {
                            code: UNRECOGNIZED_CHUNK_TYPE,
                            raw: chunk.marshal()?,
                        }],
                    };
                    let packet = Packet {
                        verification_tag: self.peer_verification_tag,
                        source_port: self.source_port,
                        destination_port: self.destination_port,
                        chunks: vec![Box::new(err_chunk)],
                    };
                    self.control_queue.push_back(packet);
                    self.awake_write_loop();
                    return Err(Error::ErrChunkTypeUnhandled);
                }
                0b10 => {
                    // just ignore
                    vec![]
                }
                0b11 => {
                    // keep processing but report the chunk as unrecognized
                    let err_chunk = ChunkError {
                        error_causes: vec![ErrorCause {
                            code: UNRECOGNIZED_CHUNK_TYPE,
                            raw: chunk.marshal()?,
                        }],
                    };
                    let packet = Packet {
                        verification_tag: self.peer_verification_tag,
                        source_port: self.source_port,
                        destination_port: self.destination_port,
                        chunks: vec![Box::new(err_chunk)],
                    };
                    vec![packet]
                }
                _ => unreachable!("This can only have 4 values."),
            }
        };

        if !packets.is_empty() {
            let mut buf: VecDeque<_> = packets.into_iter().collect();
            self.control_queue.append(&mut buf);
            self.awake_write_loop();
        }

        Ok(())
    }

    /// buffered_amount returns total amount (in bytes) of currently buffered user data.
    /// This is used only by testing.
    pub(crate) fn buffered_amount(&self) -> usize {
        self.pending_queue.get_num_bytes() + self.inflight_queue.get_num_bytes()
    }
}

#[async_trait]
impl AckTimerObserver for AssociationInternal {
    async fn on_ack_timeout(&mut self) {
        log::trace!(
            "[{}] ack timed out (ack_state: {})",
            self.name,
            self.ack_state
        );
        self.stats.inc_ack_timeouts();
        self.ack_state = AckState::Immediate;
        self.awake_write_loop();
    }
}

#[async_trait]
impl RtxTimerObserver for AssociationInternal {
    async fn on_retransmission_timeout(&mut self, id: RtxTimerId, n_rtos: usize) {
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
                        self.will_send_shutdown.store(true, Ordering::SeqCst);
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
                    let mut i = self.advanced_peer_tsn_ack_point + 1;
                    while let Some(c) = self.inflight_queue.get(i) {
                        if !c.abandoned() {
                            break;
                        }
                        self.advanced_peer_tsn_ack_point = i;
                        i += 1;
                    }

                    // RFC 3758 Sec 3.5 C3
                    if sna32gt(
                        self.advanced_peer_tsn_ack_point,
                        self.cumulative_tsn_ack_point,
                    ) {
                        self.will_send_forward_tsn = true;
                        log::debug!(
                            "[{}] on_retransmission_timeout {}: sna32GT({}, {})",
                            self.name,
                            self.will_send_forward_tsn,
                            self.advanced_peer_tsn_ack_point,
                            self.cumulative_tsn_ack_point
                        );
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

    async fn on_retransmission_failure(&mut self, id: RtxTimerId) {
        match id {
            RtxTimerId::T1Init => {
                log::error!("[{}] retransmission failure: T1-init", self.name);
                if let Some(handshake_completed_ch) = &self.handshake_completed_ch_tx {
                    let _ = handshake_completed_ch
                        .send(Some(Error::ErrHandshakeInitAck))
                        .await;
                }
            }
            RtxTimerId::T1Cookie => {
                log::error!("[{}] retransmission failure: T1-cookie", self.name);
                if let Some(handshake_completed_ch) = &self.handshake_completed_ch_tx {
                    let _ = handshake_completed_ch
                        .send(Some(Error::ErrHandshakeCookieEcho))
                        .await;
                }
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
}
