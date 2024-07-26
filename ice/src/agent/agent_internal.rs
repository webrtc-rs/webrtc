use portable_atomic::{AtomicBool, AtomicU64};

use arc_swap::ArcSwapOption;
use util::sync::Mutex as SyncMutex;

use super::agent_transport::*;
use super::*;
use crate::candidate::candidate_base::CandidateBaseConfig;
use crate::candidate::candidate_peer_reflexive::CandidatePeerReflexiveConfig;
use crate::util::*;

pub type ChanCandidateTx =
    Arc<Mutex<Option<mpsc::Sender<Option<Arc<dyn Candidate + Send + Sync>>>>>>;

#[derive(Default)]
pub(crate) struct UfragPwd {
    pub(crate) local_ufrag: String,
    pub(crate) local_pwd: String,
    pub(crate) remote_ufrag: String,
    pub(crate) remote_pwd: String,
}

pub struct AgentInternal {
    // State owned by the taskLoop
    pub(crate) on_connected_tx: Mutex<Option<mpsc::Sender<()>>>,
    pub(crate) on_connected_rx: Mutex<Option<mpsc::Receiver<()>>>,

    // State for closing
    pub(crate) done_tx: Mutex<Option<mpsc::Sender<()>>>,
    // force candidate to be contacted immediately (instead of waiting for task ticker)
    pub(crate) force_candidate_contact_tx: mpsc::Sender<bool>,
    pub(crate) done_and_force_candidate_contact_rx:
        Mutex<Option<(mpsc::Receiver<()>, mpsc::Receiver<bool>)>>,

    pub(crate) chan_candidate_tx: ChanCandidateTx,
    pub(crate) chan_candidate_pair_tx: Mutex<Option<mpsc::Sender<()>>>,
    pub(crate) chan_state_tx: Mutex<Option<mpsc::Sender<ConnectionState>>>,

    pub(crate) on_connection_state_change_hdlr: ArcSwapOption<Mutex<OnConnectionStateChangeHdlrFn>>,
    pub(crate) on_selected_candidate_pair_change_hdlr:
        ArcSwapOption<Mutex<OnSelectedCandidatePairChangeHdlrFn>>,
    pub(crate) on_candidate_hdlr: ArcSwapOption<Mutex<OnCandidateHdlrFn>>,

    pub(crate) tie_breaker: AtomicU64,
    pub(crate) is_controlling: AtomicBool,
    pub(crate) lite: AtomicBool,

    pub(crate) start_time: SyncMutex<Instant>,
    pub(crate) nominated_pair: Mutex<Option<Arc<CandidatePair>>>,

    pub(crate) connection_state: AtomicU8, //ConnectionState,

    pub(crate) started_ch_tx: Mutex<Option<broadcast::Sender<()>>>,

    pub(crate) ufrag_pwd: Mutex<UfragPwd>,

    pub(crate) local_candidates: Mutex<HashMap<NetworkType, Vec<Arc<dyn Candidate + Send + Sync>>>>,
    pub(crate) remote_candidates:
        Mutex<HashMap<NetworkType, Vec<Arc<dyn Candidate + Send + Sync>>>>,

    // LRU of outbound Binding request Transaction IDs
    pub(crate) pending_binding_requests: Mutex<Vec<BindingRequest>>,

    pub(crate) agent_conn: Arc<AgentConn>,

    // the following variables won't be changed after init_with_defaults()
    pub(crate) insecure_skip_verify: bool,
    pub(crate) max_binding_requests: u16,
    pub(crate) host_acceptance_min_wait: Duration,
    pub(crate) srflx_acceptance_min_wait: Duration,
    pub(crate) prflx_acceptance_min_wait: Duration,
    pub(crate) relay_acceptance_min_wait: Duration,
    // How long connectivity checks can fail before the ICE Agent
    // goes to disconnected
    pub(crate) disconnected_timeout: Duration,
    // How long connectivity checks can fail before the ICE Agent
    // goes to failed
    pub(crate) failed_timeout: Duration,
    // How often should we send keepalive packets?
    // 0 means never
    pub(crate) keepalive_interval: Duration,
    // How often should we run our internal taskLoop to check for state changes when connecting
    pub(crate) check_interval: Duration,
}

impl AgentInternal {
    pub(super) fn new(config: &AgentConfig) -> (Self, ChanReceivers) {
        let (chan_state_tx, chan_state_rx) = mpsc::channel(1);
        let (chan_candidate_tx, chan_candidate_rx) = mpsc::channel(1);
        let (chan_candidate_pair_tx, chan_candidate_pair_rx) = mpsc::channel(1);
        let (on_connected_tx, on_connected_rx) = mpsc::channel(1);
        let (done_tx, done_rx) = mpsc::channel(1);
        let (force_candidate_contact_tx, force_candidate_contact_rx) = mpsc::channel(1);
        let (started_ch_tx, _) = broadcast::channel(1);

        let ai = AgentInternal {
            on_connected_tx: Mutex::new(Some(on_connected_tx)),
            on_connected_rx: Mutex::new(Some(on_connected_rx)),

            done_tx: Mutex::new(Some(done_tx)),
            force_candidate_contact_tx,
            done_and_force_candidate_contact_rx: Mutex::new(Some((
                done_rx,
                force_candidate_contact_rx,
            ))),

            chan_candidate_tx: Arc::new(Mutex::new(Some(chan_candidate_tx))),
            chan_candidate_pair_tx: Mutex::new(Some(chan_candidate_pair_tx)),
            chan_state_tx: Mutex::new(Some(chan_state_tx)),

            on_connection_state_change_hdlr: ArcSwapOption::empty(),
            on_selected_candidate_pair_change_hdlr: ArcSwapOption::empty(),
            on_candidate_hdlr: ArcSwapOption::empty(),

            tie_breaker: AtomicU64::new(rand::random::<u64>()),
            is_controlling: AtomicBool::new(config.is_controlling),
            lite: AtomicBool::new(config.lite),

            start_time: SyncMutex::new(Instant::now()),
            nominated_pair: Mutex::new(None),

            connection_state: AtomicU8::new(ConnectionState::New as u8),

            insecure_skip_verify: config.insecure_skip_verify,

            started_ch_tx: Mutex::new(Some(started_ch_tx)),

            //won't change after init_with_defaults()
            max_binding_requests: 0,
            host_acceptance_min_wait: Duration::from_secs(0),
            srflx_acceptance_min_wait: Duration::from_secs(0),
            prflx_acceptance_min_wait: Duration::from_secs(0),
            relay_acceptance_min_wait: Duration::from_secs(0),

            // How long connectivity checks can fail before the ICE Agent
            // goes to disconnected
            disconnected_timeout: Duration::from_secs(0),

            // How long connectivity checks can fail before the ICE Agent
            // goes to failed
            failed_timeout: Duration::from_secs(0),

            // How often should we send keepalive packets?
            // 0 means never
            keepalive_interval: Duration::from_secs(0),

            // How often should we run our internal taskLoop to check for state changes when connecting
            check_interval: Duration::from_secs(0),

            ufrag_pwd: Mutex::new(UfragPwd::default()),

            local_candidates: Mutex::new(HashMap::new()),
            remote_candidates: Mutex::new(HashMap::new()),

            // LRU of outbound Binding request Transaction IDs
            pending_binding_requests: Mutex::new(vec![]),

            // AgentConn
            agent_conn: Arc::new(AgentConn::new()),
        };

        let chan_receivers = ChanReceivers {
            chan_state_rx,
            chan_candidate_rx,
            chan_candidate_pair_rx,
        };
        (ai, chan_receivers)
    }
    pub(crate) async fn start_connectivity_checks(
        self: &Arc<Self>,
        is_controlling: bool,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<()> {
        {
            let started_ch_tx = self.started_ch_tx.lock().await;
            if started_ch_tx.is_none() {
                return Err(Error::ErrMultipleStart);
            }
        }

        log::debug!(
            "Started agent: isControlling? {}, remoteUfrag: {}, remotePwd: {}",
            is_controlling,
            remote_ufrag,
            remote_pwd
        );
        self.set_remote_credentials(remote_ufrag, remote_pwd)
            .await?;
        self.is_controlling.store(is_controlling, Ordering::SeqCst);
        self.start().await;
        {
            let mut started_ch_tx = self.started_ch_tx.lock().await;
            started_ch_tx.take();
        }

        self.update_connection_state(ConnectionState::Checking)
            .await;

        self.request_connectivity_check();

        self.connectivity_checks().await;

        Ok(())
    }

    async fn contact(
        &self,
        last_connection_state: &mut ConnectionState,
        checking_duration: &mut Instant,
    ) {
        if self.connection_state.load(Ordering::SeqCst) == ConnectionState::Failed as u8 {
            // The connection is currently failed so don't send any checks
            // In the future it may be restarted though
            *last_connection_state = self.connection_state.load(Ordering::SeqCst).into();
            return;
        }
        if self.connection_state.load(Ordering::SeqCst) == ConnectionState::Checking as u8 {
            // We have just entered checking for the first time so update our checking timer
            if *last_connection_state as u8 != self.connection_state.load(Ordering::SeqCst) {
                *checking_duration = Instant::now();
            }

            // We have been in checking longer then Disconnect+Failed timeout, set the connection to Failed
            if Instant::now()
                .checked_duration_since(*checking_duration)
                .unwrap_or_else(|| Duration::from_secs(0))
                > self.disconnected_timeout + self.failed_timeout
            {
                self.update_connection_state(ConnectionState::Failed).await;
                *last_connection_state = self.connection_state.load(Ordering::SeqCst).into();
                return;
            }
        }

        self.contact_candidates().await;

        *last_connection_state = self.connection_state.load(Ordering::SeqCst).into();
    }

    async fn connectivity_checks(self: &Arc<Self>) {
        const ZERO_DURATION: Duration = Duration::from_secs(0);
        let mut last_connection_state = ConnectionState::Unspecified;
        let mut checking_duration = Instant::now();
        let (check_interval, keepalive_interval, disconnected_timeout, failed_timeout) = (
            self.check_interval,
            self.keepalive_interval,
            self.disconnected_timeout,
            self.failed_timeout,
        );

        let done_and_force_candidate_contact_rx = {
            let mut done_and_force_candidate_contact_rx =
                self.done_and_force_candidate_contact_rx.lock().await;
            done_and_force_candidate_contact_rx.take()
        };

        if let Some((mut done_rx, mut force_candidate_contact_rx)) =
            done_and_force_candidate_contact_rx
        {
            let ai = Arc::clone(self);
            tokio::spawn(async move {
                loop {
                    let mut interval = DEFAULT_CHECK_INTERVAL;

                    let mut update_interval = |x: Duration| {
                        if x != ZERO_DURATION && (interval == ZERO_DURATION || interval > x) {
                            interval = x;
                        }
                    };

                    match last_connection_state {
                        ConnectionState::New | ConnectionState::Checking => {
                            // While connecting, check candidates more frequently
                            update_interval(check_interval);
                        }
                        ConnectionState::Connected | ConnectionState::Disconnected => {
                            update_interval(keepalive_interval);
                        }
                        _ => {}
                    };
                    // Ensure we run our task loop as quickly as the minimum of our various configured timeouts
                    update_interval(disconnected_timeout);
                    update_interval(failed_timeout);

                    let t = tokio::time::sleep(interval);
                    tokio::pin!(t);

                    tokio::select! {
                        _ = t.as_mut() => {
                            ai.contact(&mut last_connection_state, &mut checking_duration).await;
                        },
                        _ = force_candidate_contact_rx.recv() => {
                            ai.contact(&mut last_connection_state, &mut checking_duration).await;
                        },
                        _ = done_rx.recv() => {
                            return;
                        }
                    }
                }
            });
        }
    }

    pub(crate) async fn update_connection_state(&self, new_state: ConnectionState) {
        if self.connection_state.load(Ordering::SeqCst) != new_state as u8 {
            // Connection has gone to failed, release all gathered candidates
            if new_state == ConnectionState::Failed {
                self.delete_all_candidates().await;
            }

            log::info!(
                "[{}]: Setting new connection state: {}",
                self.get_name(),
                new_state
            );
            self.connection_state
                .store(new_state as u8, Ordering::SeqCst);

            // Call handler after finishing current task since we may be holding the agent lock
            // and the handler may also require it
            {
                let chan_state_tx = self.chan_state_tx.lock().await;
                if let Some(tx) = &*chan_state_tx {
                    let _ = tx.send(new_state).await;
                }
            }
        }
    }

    pub(crate) async fn set_selected_pair(&self, p: Option<Arc<CandidatePair>>) {
        log::trace!(
            "[{}]: Set selected candidate pair: {:?}",
            self.get_name(),
            p
        );

        if let Some(p) = p {
            p.nominated.store(true, Ordering::SeqCst);
            self.agent_conn.selected_pair.store(Some(p));

            self.update_connection_state(ConnectionState::Connected)
                .await;

            // Notify when the selected pair changes
            {
                let chan_candidate_pair_tx = self.chan_candidate_pair_tx.lock().await;
                if let Some(tx) = &*chan_candidate_pair_tx {
                    let _ = tx.send(()).await;
                }
            }

            // Signal connected
            {
                let mut on_connected_tx = self.on_connected_tx.lock().await;
                on_connected_tx.take();
            }
        } else {
            self.agent_conn.selected_pair.store(None);
        }
    }

    pub(crate) async fn ping_all_candidates(&self) {
        log::trace!("[{}]: pinging all candidates", self.get_name(),);

        let mut pairs: Vec<(
            Arc<dyn Candidate + Send + Sync>,
            Arc<dyn Candidate + Send + Sync>,
        )> = vec![];

        {
            let mut checklist = self.agent_conn.checklist.lock().await;
            if checklist.is_empty() {
                log::warn!(
                    "[{}]: pingAllCandidates called with no candidate pairs. Connection is not possible yet.",
                    self.get_name(),
                );
            }
            for p in &mut *checklist {
                let p_state = p.state.load(Ordering::SeqCst);
                if p_state == CandidatePairState::Waiting as u8 {
                    p.state
                        .store(CandidatePairState::InProgress as u8, Ordering::SeqCst);
                } else if p_state != CandidatePairState::InProgress as u8 {
                    continue;
                }

                if p.binding_request_count.load(Ordering::SeqCst) > self.max_binding_requests {
                    log::trace!(
                        "[{}]: max requests reached for pair {}, marking it as failed",
                        self.get_name(),
                        p
                    );
                    p.state
                        .store(CandidatePairState::Failed as u8, Ordering::SeqCst);
                } else {
                    p.binding_request_count.fetch_add(1, Ordering::SeqCst);
                    let local = p.local.clone();
                    let remote = p.remote.clone();
                    pairs.push((local, remote));
                }
            }
        }

        for (local, remote) in pairs {
            self.ping_candidate(&local, &remote).await;
        }
    }

    pub(crate) async fn add_pair(
        &self,
        local: Arc<dyn Candidate + Send + Sync>,
        remote: Arc<dyn Candidate + Send + Sync>,
    ) {
        let p = Arc::new(CandidatePair::new(
            local,
            remote,
            self.is_controlling.load(Ordering::SeqCst),
        ));
        let mut checklist = self.agent_conn.checklist.lock().await;
        checklist.push(p);
    }

    pub(crate) async fn find_pair(
        &self,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) -> Option<Arc<CandidatePair>> {
        let checklist = self.agent_conn.checklist.lock().await;
        for p in &*checklist {
            if p.local.equal(&**local) && p.remote.equal(&**remote) {
                return Some(p.clone());
            }
        }
        None
    }

    /// Checks if the selected pair is (still) valid.
    /// Note: the caller should hold the agent lock.
    pub(crate) async fn validate_selected_pair(&self) -> bool {
        let (valid, disconnected_time) = {
            let selected_pair = self.agent_conn.selected_pair.load();
            (*selected_pair).as_ref().map_or_else(
                || (false, Duration::from_secs(0)),
                |selected_pair| {
                    let disconnected_time = SystemTime::now()
                        .duration_since(selected_pair.remote.last_received())
                        .unwrap_or_else(|_| Duration::from_secs(0));
                    (true, disconnected_time)
                },
            )
        };

        if valid {
            // Only allow transitions to failed if a.failedTimeout is non-zero
            let mut total_time_to_failure = self.failed_timeout;
            if total_time_to_failure != Duration::from_secs(0) {
                total_time_to_failure += self.disconnected_timeout;
            }

            if total_time_to_failure != Duration::from_secs(0)
                && disconnected_time > total_time_to_failure
            {
                self.update_connection_state(ConnectionState::Failed).await;
            } else if self.disconnected_timeout != Duration::from_secs(0)
                && disconnected_time > self.disconnected_timeout
            {
                self.update_connection_state(ConnectionState::Disconnected)
                    .await;
            } else {
                self.update_connection_state(ConnectionState::Connected)
                    .await;
            }
        }

        valid
    }

    /// Sends STUN Binding Indications to the selected pair.
    /// if no packet has been sent on that pair in the last keepaliveInterval.
    /// Note: the caller should hold the agent lock.
    pub(crate) async fn check_keepalive(&self) {
        let (local, remote) = {
            let selected_pair = self.agent_conn.selected_pair.load();
            (*selected_pair)
                .as_ref()
                .map_or((None, None), |selected_pair| {
                    (
                        Some(selected_pair.local.clone()),
                        Some(selected_pair.remote.clone()),
                    )
                })
        };

        if let (Some(local), Some(remote)) = (local, remote) {
            let last_sent = SystemTime::now()
                .duration_since(local.last_sent())
                .unwrap_or_else(|_| Duration::from_secs(0));

            let last_received = SystemTime::now()
                .duration_since(remote.last_received())
                .unwrap_or_else(|_| Duration::from_secs(0));

            if (self.keepalive_interval != Duration::from_secs(0))
                && ((last_sent > self.keepalive_interval)
                    || (last_received > self.keepalive_interval))
            {
                // we use binding request instead of indication to support refresh consent schemas
                // see https://tools.ietf.org/html/rfc7675
                self.ping_candidate(&local, &remote).await;
            }
        }
    }

    fn request_connectivity_check(&self) {
        let _ = self.force_candidate_contact_tx.try_send(true);
    }

    /// Assumes you are holding the lock (must be execute using a.run).
    pub(crate) async fn add_remote_candidate(&self, c: &Arc<dyn Candidate + Send + Sync>) {
        let network_type = c.network_type();

        {
            let mut remote_candidates = self.remote_candidates.lock().await;
            if let Some(cands) = remote_candidates.get(&network_type) {
                for cand in cands {
                    if cand.equal(&**c) {
                        return;
                    }
                }
            }

            if let Some(cands) = remote_candidates.get_mut(&network_type) {
                cands.push(c.clone());
            } else {
                remote_candidates.insert(network_type, vec![c.clone()]);
            }
        }

        let mut local_cands = vec![];
        {
            let local_candidates = self.local_candidates.lock().await;
            if let Some(cands) = local_candidates.get(&network_type) {
                local_cands.clone_from(cands);
            }
        }

        for cand in local_cands {
            self.add_pair(cand, c.clone()).await;
        }

        self.request_connectivity_check();
    }

    pub(crate) async fn add_candidate(
        self: &Arc<Self>,
        c: &Arc<dyn Candidate + Send + Sync>,
    ) -> Result<()> {
        let initialized_ch = {
            let started_ch_tx = self.started_ch_tx.lock().await;
            (*started_ch_tx).as_ref().map(|tx| tx.subscribe())
        };

        self.start_candidate(c, initialized_ch).await;

        let network_type = c.network_type();
        {
            let mut local_candidates = self.local_candidates.lock().await;
            if let Some(cands) = local_candidates.get(&network_type) {
                for cand in cands {
                    if cand.equal(&**c) {
                        if let Err(err) = c.close().await {
                            log::warn!(
                                "[{}]: Failed to close duplicate candidate: {}",
                                self.get_name(),
                                err
                            );
                        }
                        //TODO: why return?
                        return Ok(());
                    }
                }
            }

            if let Some(cands) = local_candidates.get_mut(&network_type) {
                cands.push(c.clone());
            } else {
                local_candidates.insert(network_type, vec![c.clone()]);
            }
        }

        let mut remote_cands = vec![];
        {
            let remote_candidates = self.remote_candidates.lock().await;
            if let Some(cands) = remote_candidates.get(&network_type) {
                remote_cands.clone_from(cands);
            }
        }

        for cand in remote_cands {
            self.add_pair(c.clone(), cand).await;
        }

        self.request_connectivity_check();
        {
            let chan_candidate_tx = self.chan_candidate_tx.lock().await;
            if let Some(tx) = &*chan_candidate_tx {
                let _ = tx.send(Some(c.clone())).await;
            }
        }

        Ok(())
    }

    pub(crate) async fn close(&self) -> Result<()> {
        {
            let mut done_tx = self.done_tx.lock().await;
            if done_tx.is_none() {
                return Err(Error::ErrClosed);
            }
            done_tx.take();
        };
        self.delete_all_candidates().await;
        {
            let mut started_ch_tx = self.started_ch_tx.lock().await;
            started_ch_tx.take();
        }

        self.agent_conn.buffer.close().await;

        self.update_connection_state(ConnectionState::Closed).await;

        {
            let mut chan_candidate_tx = self.chan_candidate_tx.lock().await;
            chan_candidate_tx.take();
        }
        {
            let mut chan_candidate_pair_tx = self.chan_candidate_pair_tx.lock().await;
            chan_candidate_pair_tx.take();
        }
        {
            let mut chan_state_tx = self.chan_state_tx.lock().await;
            chan_state_tx.take();
        }

        self.agent_conn.done.store(true, Ordering::SeqCst);

        Ok(())
    }

    /// Remove all candidates.
    /// This closes any listening sockets and removes both the local and remote candidate lists.
    ///
    /// This is used for restarts, failures and on close.
    pub(crate) async fn delete_all_candidates(&self) {
        {
            let mut local_candidates = self.local_candidates.lock().await;
            for cs in local_candidates.values_mut() {
                for c in cs {
                    if let Err(err) = c.close().await {
                        log::warn!(
                            "[{}]: Failed to close candidate {}: {}",
                            self.get_name(),
                            c,
                            err
                        );
                    }
                }
            }
            local_candidates.clear();
        }

        {
            let mut remote_candidates = self.remote_candidates.lock().await;
            for cs in remote_candidates.values_mut() {
                for c in cs {
                    if let Err(err) = c.close().await {
                        log::warn!(
                            "[{}]: Failed to close candidate {}: {}",
                            self.get_name(),
                            c,
                            err
                        );
                    }
                }
            }
            remote_candidates.clear();
        }
    }

    pub(crate) async fn find_remote_candidate(
        &self,
        network_type: NetworkType,
        addr: SocketAddr,
    ) -> Option<Arc<dyn Candidate + Send + Sync>> {
        let (ip, port) = (addr.ip(), addr.port());

        let remote_candidates = self.remote_candidates.lock().await;
        if let Some(cands) = remote_candidates.get(&network_type) {
            for c in cands {
                if c.address() == ip.to_string() && c.port() == port {
                    return Some(c.clone());
                }
            }
        }
        None
    }

    pub(crate) async fn send_binding_request(
        &self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        log::trace!(
            "[{}]: ping STUN from {} to {}",
            self.get_name(),
            local,
            remote
        );

        self.invalidate_pending_binding_requests(Instant::now())
            .await;
        {
            let mut pending_binding_requests = self.pending_binding_requests.lock().await;
            pending_binding_requests.push(BindingRequest {
                timestamp: Instant::now(),
                transaction_id: m.transaction_id,
                destination: remote.addr(),
                is_use_candidate: m.contains(ATTR_USE_CANDIDATE),
            });
        }

        self.send_stun(m, local, remote).await;
    }

    pub(crate) async fn send_binding_success(
        &self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        let addr = remote.addr();
        let (ip, port) = (addr.ip(), addr.port());
        let local_pwd = {
            let ufrag_pwd = self.ufrag_pwd.lock().await;
            ufrag_pwd.local_pwd.clone()
        };

        let (out, result) = {
            let mut out = Message::new();
            let result = out.build(&[
                Box::new(m.clone()),
                Box::new(BINDING_SUCCESS),
                Box::new(XorMappedAddress { ip, port }),
                Box::new(MessageIntegrity::new_short_term_integrity(local_pwd)),
                Box::new(FINGERPRINT),
            ]);
            (out, result)
        };

        if let Err(err) = result {
            log::warn!(
                "[{}]: Failed to handle inbound ICE from: {} to: {} error: {}",
                self.get_name(),
                local,
                remote,
                err
            );
        } else {
            self.send_stun(&out, local, remote).await;
        }
    }

    /// Removes pending binding requests that are over `maxBindingRequestTimeout` old Let HTO be the
    /// transaction timeout, which SHOULD be 2*RTT if RTT is known or 500 ms otherwise.
    ///
    /// reference: (IETF ref-8445)[https://tools.ietf.org/html/rfc8445#appendix-B.1].
    pub(crate) async fn invalidate_pending_binding_requests(&self, filter_time: Instant) {
        let mut pending_binding_requests = self.pending_binding_requests.lock().await;
        let initial_size = pending_binding_requests.len();

        let mut temp = vec![];
        for binding_request in pending_binding_requests.drain(..) {
            if filter_time
                .checked_duration_since(binding_request.timestamp)
                .map(|duration| duration < MAX_BINDING_REQUEST_TIMEOUT)
                .unwrap_or(true)
            {
                temp.push(binding_request);
            }
        }

        *pending_binding_requests = temp;
        let bind_requests_removed = initial_size - pending_binding_requests.len();
        if bind_requests_removed > 0 {
            log::trace!(
                "[{}]: Discarded {} binding requests because they expired",
                self.get_name(),
                bind_requests_removed
            );
        }
    }

    /// Assert that the passed `TransactionID` is in our `pendingBindingRequests` and returns the
    /// destination, If the bindingRequest was valid remove it from our pending cache.
    pub(crate) async fn handle_inbound_binding_success(
        &self,
        id: TransactionId,
    ) -> Option<BindingRequest> {
        self.invalidate_pending_binding_requests(Instant::now())
            .await;

        let mut pending_binding_requests = self.pending_binding_requests.lock().await;
        for i in 0..pending_binding_requests.len() {
            if pending_binding_requests[i].transaction_id == id {
                let valid_binding_request = pending_binding_requests.remove(i);
                return Some(valid_binding_request);
            }
        }
        None
    }

    /// Processes STUN traffic from a remote candidate.
    pub(crate) async fn handle_inbound(
        &self,
        m: &mut Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: SocketAddr,
    ) {
        if m.typ.method != METHOD_BINDING
            || !(m.typ.class == CLASS_SUCCESS_RESPONSE
                || m.typ.class == CLASS_REQUEST
                || m.typ.class == CLASS_INDICATION)
        {
            log::trace!(
                "[{}]: unhandled STUN from {} to {} class({}) method({})",
                self.get_name(),
                remote,
                local,
                m.typ.class,
                m.typ.method
            );
            return;
        }

        if self.is_controlling.load(Ordering::SeqCst) {
            if m.contains(ATTR_ICE_CONTROLLING) {
                log::debug!(
                    "[{}]: inbound isControlling && a.isControlling == true",
                    self.get_name(),
                );
                return;
            } else if m.contains(ATTR_USE_CANDIDATE) {
                log::debug!(
                    "[{}]: useCandidate && a.isControlling == true",
                    self.get_name(),
                );
                return;
            }
        } else if m.contains(ATTR_ICE_CONTROLLED) {
            log::debug!(
                "[{}]: inbound isControlled && a.isControlling == false",
                self.get_name(),
            );
            return;
        }

        let mut remote_candidate = self
            .find_remote_candidate(local.network_type(), remote)
            .await;
        if m.typ.class == CLASS_SUCCESS_RESPONSE {
            {
                let ufrag_pwd = self.ufrag_pwd.lock().await;
                if let Err(err) =
                    assert_inbound_message_integrity(m, ufrag_pwd.remote_pwd.as_bytes())
                {
                    log::warn!(
                        "[{}]: discard message from ({}), {}",
                        self.get_name(),
                        remote,
                        err
                    );
                    return;
                }
            }

            if let Some(rc) = &remote_candidate {
                self.handle_success_response(m, local, rc, remote).await;
            } else {
                log::warn!(
                    "[{}]: discard success message from ({}), no such remote",
                    self.get_name(),
                    remote
                );
                return;
            }
        } else if m.typ.class == CLASS_REQUEST {
            {
                let ufrag_pwd = self.ufrag_pwd.lock().await;
                let username =
                    ufrag_pwd.local_ufrag.clone() + ":" + ufrag_pwd.remote_ufrag.as_str();
                if let Err(err) = assert_inbound_username(m, &username) {
                    log::warn!(
                        "[{}]: discard message from ({}), {}",
                        self.get_name(),
                        remote,
                        err
                    );
                    return;
                } else if let Err(err) =
                    assert_inbound_message_integrity(m, ufrag_pwd.local_pwd.as_bytes())
                {
                    log::warn!(
                        "[{}]: discard message from ({}), {}",
                        self.get_name(),
                        remote,
                        err
                    );
                    return;
                }
            }

            if remote_candidate.is_none() {
                let (ip, port, network_type) = (remote.ip(), remote.port(), NetworkType::Udp4);

                let prflx_candidate_config = CandidatePeerReflexiveConfig {
                    base_config: CandidateBaseConfig {
                        network: network_type.to_string(),
                        address: ip.to_string(),
                        port,
                        component: local.component(),
                        ..CandidateBaseConfig::default()
                    },
                    rel_addr: "".to_owned(),
                    rel_port: 0,
                };

                match prflx_candidate_config.new_candidate_peer_reflexive() {
                    Ok(prflx_candidate) => remote_candidate = Some(Arc::new(prflx_candidate)),
                    Err(err) => {
                        log::error!(
                            "[{}]: Failed to create new remote prflx candidate ({})",
                            self.get_name(),
                            err
                        );
                        return;
                    }
                };

                log::debug!(
                    "[{}]: adding a new peer-reflexive candidate: {} ",
                    self.get_name(),
                    remote
                );
                if let Some(rc) = &remote_candidate {
                    self.add_remote_candidate(rc).await;
                }
            }

            log::trace!(
                "[{}]: inbound STUN (Request) from {} to {}",
                self.get_name(),
                remote,
                local
            );

            if let Some(rc) = &remote_candidate {
                self.handle_binding_request(m, local, rc).await;
            }
        }

        if let Some(rc) = remote_candidate {
            rc.seen(false);
        }
    }

    /// Processes non STUN traffic from a remote candidate, and returns true if it is an actual
    /// remote candidate.
    pub(crate) async fn validate_non_stun_traffic(
        &self,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: SocketAddr,
    ) -> bool {
        self.find_remote_candidate(local.network_type(), remote)
            .await
            .map_or(false, |remote_candidate| {
                remote_candidate.seen(false);
                true
            })
    }

    /// Sets the credentials of the remote agent.
    pub(crate) async fn set_remote_credentials(
        &self,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<()> {
        if remote_ufrag.is_empty() {
            return Err(Error::ErrRemoteUfragEmpty);
        } else if remote_pwd.is_empty() {
            return Err(Error::ErrRemotePwdEmpty);
        }

        let mut ufrag_pwd = self.ufrag_pwd.lock().await;
        ufrag_pwd.remote_ufrag = remote_ufrag;
        ufrag_pwd.remote_pwd = remote_pwd;
        Ok(())
    }

    pub(crate) async fn send_stun(
        &self,
        msg: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        if let Err(err) = local.write_to(&msg.raw, &**remote).await {
            log::trace!(
                "[{}]: failed to send STUN message: {}",
                self.get_name(),
                err
            );
        }
    }

    /// Runs the candidate using the provided connection.
    async fn start_candidate(
        self: &Arc<Self>,
        candidate: &Arc<dyn Candidate + Send + Sync>,
        initialized_ch: Option<broadcast::Receiver<()>>,
    ) {
        let (closed_ch_tx, closed_ch_rx) = broadcast::channel(1);
        {
            let closed_ch = candidate.get_closed_ch();
            let mut closed = closed_ch.lock().await;
            *closed = Some(closed_ch_tx);
        }

        let cand = Arc::clone(candidate);
        if let Some(conn) = candidate.get_conn() {
            let conn = Arc::clone(conn);
            let addr = candidate.addr();
            let ai = Arc::clone(self);
            tokio::spawn(async move {
                let _ = ai
                    .recv_loop(cand, closed_ch_rx, initialized_ch, conn, addr)
                    .await;
            });
        } else {
            log::error!("[{}]: Can't start due to conn is_none", self.get_name(),);
        }
    }

    pub(super) fn start_on_connection_state_change_routine(
        self: &Arc<Self>,
        mut chan_state_rx: mpsc::Receiver<ConnectionState>,
        mut chan_candidate_rx: mpsc::Receiver<Option<Arc<dyn Candidate + Send + Sync>>>,
        mut chan_candidate_pair_rx: mpsc::Receiver<()>,
    ) {
        let ai = Arc::clone(self);
        tokio::spawn(async move {
            // CandidatePair and ConnectionState are usually changed at once.
            // Blocking one by the other one causes deadlock.
            while chan_candidate_pair_rx.recv().await.is_some() {
                if let (Some(cb), Some(p)) = (
                    &*ai.on_selected_candidate_pair_change_hdlr.load(),
                    &*ai.agent_conn.selected_pair.load(),
                ) {
                    let mut f = cb.lock().await;
                    f(&p.local, &p.remote).await;
                }
            }
        });

        let ai = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    opt_state = chan_state_rx.recv() => {
                        if let Some(s) = opt_state {
                            if let Some(handler) = &*ai.on_connection_state_change_hdlr.load() {
                                let mut f = handler.lock().await;
                                f(s).await;
                            }
                        } else {
                            while let Some(c) = chan_candidate_rx.recv().await {
                                if let Some(handler) = &*ai.on_candidate_hdlr.load() {
                                    let mut f = handler.lock().await;
                                    f(c).await;
                                }
                            }
                            break;
                        }
                    },
                    opt_cand = chan_candidate_rx.recv() => {
                        if let Some(c) = opt_cand {
                            if let Some(handler) = &*ai.on_candidate_hdlr.load() {
                                let mut f = handler.lock().await;
                                f(c).await;
                            }
                        } else {
                            while let Some(s) = chan_state_rx.recv().await {
                                if let Some(handler) = &*ai.on_connection_state_change_hdlr.load() {
                                    let mut f = handler.lock().await;
                                    f(s).await;
                                }
                            }
                            break;
                        }
                    }
                }
            }
        });
    }

    async fn recv_loop(
        self: &Arc<Self>,
        candidate: Arc<dyn Candidate + Send + Sync>,
        mut closed_ch_rx: broadcast::Receiver<()>,
        initialized_ch: Option<broadcast::Receiver<()>>,
        conn: Arc<dyn util::Conn + Send + Sync>,
        addr: SocketAddr,
    ) -> Result<()> {
        if let Some(mut initialized_ch) = initialized_ch {
            tokio::select! {
                _ = initialized_ch.recv() => {}
                _ = closed_ch_rx.recv() => return Err(Error::ErrClosed),
            }
        }

        let mut buffer = vec![0_u8; RECEIVE_MTU];
        let mut n;
        let mut src_addr;
        loop {
            tokio::select! {
               result = conn.recv_from(&mut buffer) => {
                   match result {
                       Ok((num, src)) => {
                            n = num;
                            src_addr = src;
                       }
                       Err(err) => return Err(Error::Other(err.to_string())),
                   }
               },
                _  = closed_ch_rx.recv() => return Err(Error::ErrClosed),
            }

            self.handle_inbound_candidate_msg(&candidate, &buffer[..n], src_addr, addr)
                .await;
        }
    }

    async fn handle_inbound_candidate_msg(
        self: &Arc<Self>,
        c: &Arc<dyn Candidate + Send + Sync>,
        buf: &[u8],
        src_addr: SocketAddr,
        addr: SocketAddr,
    ) {
        if stun::message::is_message(buf) {
            let mut m = Message {
                raw: vec![],
                ..Message::default()
            };
            // Explicitly copy raw buffer so Message can own the memory.
            m.raw.extend_from_slice(buf);

            if let Err(err) = m.decode() {
                log::warn!(
                    "[{}]: Failed to handle decode ICE from {} to {}: {}",
                    self.get_name(),
                    addr,
                    src_addr,
                    err
                );
            } else {
                self.handle_inbound(&mut m, c, src_addr).await;
            }
        } else if !self.validate_non_stun_traffic(c, src_addr).await {
            log::warn!(
                "[{}]: Discarded message, not a valid remote candidate",
                self.get_name(),
                //c.addr().await //from {}
            );
        } else if let Err(err) = self.agent_conn.buffer.write(buf).await {
            // NOTE This will return packetio.ErrFull if the buffer ever manages to fill up.
            log::warn!("[{}]: failed to write packet: {}", self.get_name(), err);
        }
    }

    pub(crate) fn get_name(&self) -> &str {
        if self.is_controlling.load(Ordering::SeqCst) {
            "controlling"
        } else {
            "controlled"
        }
    }
}
