use super::agent_transport::*;
use super::*;
use crate::candidate::candidate_base::{CandidateBase, CandidateBaseConfig};
use crate::candidate::candidate_peer_reflexive::CandidatePeerReflexiveConfig;
use crate::util::*;

pub struct AgentInternal {
    // State owned by the taskLoop
    pub(crate) on_connected_tx: Option<mpsc::Sender<()>>,
    pub(crate) on_connected_rx: Option<mpsc::Receiver<()>>,

    // State for closing
    pub(crate) done_tx: Option<mpsc::Sender<()>>,
    pub(crate) done_rx: Option<mpsc::Receiver<()>>,

    pub(crate) chan_candidate_tx: Arc<mpsc::Sender<Option<Arc<dyn Candidate + Send + Sync>>>>,
    pub(crate) chan_candidate_pair_tx: mpsc::Sender<()>,
    pub(crate) chan_state_tx: mpsc::Sender<ConnectionState>,

    pub(crate) on_connection_state_change_hdlr: Option<OnConnectionStateChangeHdlrFn>,
    pub(crate) on_selected_candidate_pair_change_hdlr: Option<OnSelectedCandidatePairChangeHdlrFn>,
    pub(crate) on_candidate_hdlr: Option<OnCandidateHdlrFn>,

    // force candidate to be contacted immediately (instead of waiting for task ticker)
    pub(crate) force_candidate_contact_tx: mpsc::Sender<bool>,
    pub(crate) force_candidate_contact_rx: Option<mpsc::Receiver<bool>>,
    pub(crate) tie_breaker: u64,

    pub(crate) is_controlling: bool,
    pub(crate) lite: bool,
    pub(crate) start_time: Instant,
    pub(crate) nominated_pair: Option<Arc<CandidatePair>>,

    pub(crate) connection_state: ConnectionState,

    pub(crate) started_ch_tx: Option<broadcast::Sender<()>>,

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

    pub(crate) local_ufrag: String,
    pub(crate) local_pwd: String,
    pub(crate) local_candidates: HashMap<NetworkType, Vec<Arc<dyn Candidate + Send + Sync>>>,

    pub(crate) remote_ufrag: String,
    pub(crate) remote_pwd: String,
    pub(crate) remote_candidates: HashMap<NetworkType, Vec<Arc<dyn Candidate + Send + Sync>>>,

    // LRU of outbound Binding request Transaction IDs
    pub(crate) pending_binding_requests: Vec<BindingRequest>,

    pub(crate) insecure_skip_verify: bool,

    pub(crate) agent_conn: Arc<AgentConn>,
}

//TODO: remove unsafe
unsafe impl Send for AgentInternal {}
unsafe impl Sync for AgentInternal {}

impl AgentInternal {
    pub(crate) async fn start_connectivity_checks(
        &mut self,
        agent_internal: Arc<Mutex<AgentInternal>>,
        is_controlling: bool,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<(), Error> {
        if self.started_ch_tx.is_none() {
            return Err(ERR_MULTIPLE_START.to_owned());
        }

        log::debug!(
            "Started agent: isControlling? {}, remoteUfrag: {}, remotePwd: {}",
            is_controlling,
            remote_ufrag,
            remote_pwd
        );
        self.set_remote_credentials(remote_ufrag, remote_pwd)?;
        self.is_controlling = is_controlling;
        self.start();
        self.started_ch_tx.take();

        self.update_connection_state(ConnectionState::Checking)
            .await;

        self.request_connectivity_check();

        self.connectivity_checks(agent_internal).await;

        Ok(())
    }

    async fn contact(
        agent_internal: &Arc<Mutex<AgentInternal>>,
        last_connection_state: &mut ConnectionState,
        checking_duration: &mut Instant,
    ) {
        let mut ai = agent_internal.lock().await;
        if ai.connection_state == ConnectionState::Failed {
            // The connection is currently failed so don't send any checks
            // In the future it may be restarted though
            *last_connection_state = ai.connection_state;
            return;
        }
        if ai.connection_state == ConnectionState::Checking {
            // We have just entered checking for the first time so update our checking timer
            if *last_connection_state != ai.connection_state {
                *checking_duration = Instant::now();
            }

            // We have been in checking longer then Disconnect+Failed timeout, set the connection to Failed
            if Instant::now().duration_since(*checking_duration)
                > ai.disconnected_timeout + ai.failed_timeout
            {
                ai.update_connection_state(ConnectionState::Failed).await;
                *last_connection_state = ai.connection_state;
                return;
            }
        }

        ai.contact_candidates().await;

        *last_connection_state = ai.connection_state;
    }

    async fn connectivity_checks(&mut self, agent_internal: Arc<Mutex<AgentInternal>>) {
        let mut last_connection_state = ConnectionState::Unspecified;
        let mut checking_duration = Instant::now();
        const ZERO_DURATION: Duration = Duration::from_secs(0);
        let (check_interval, keepalive_interval, disconnected_timeout, failed_timeout) = (
            self.check_interval,
            self.keepalive_interval,
            self.disconnected_timeout,
            self.failed_timeout,
        );

        if let (Some(mut force_candidate_contact_rx), Some(mut done_rx)) =
            (self.force_candidate_contact_rx.take(), self.done_rx.take())
        {
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
                            AgentInternal::contact(&agent_internal, &mut last_connection_state, &mut checking_duration).await;
                        },
                        _ = force_candidate_contact_rx.recv() => {
                            AgentInternal::contact(&agent_internal, &mut last_connection_state, &mut checking_duration).await;
                        },
                        _ = done_rx.recv() => {
                            return;
                        }
                    }
                }
            });
        }
    }

    pub(crate) async fn update_connection_state(&mut self, new_state: ConnectionState) {
        if self.connection_state != new_state {
            // Connection has gone to failed, release all gathered candidates
            if new_state == ConnectionState::Failed {
                self.delete_all_candidates().await;
            }

            log::info!("Setting new connection state: {}", new_state);
            self.connection_state = new_state;

            // Call handler after finishing current task since we may be holding the agent lock
            // and the handler may also require it
            let _ = self.chan_state_tx.send(new_state).await;
        }
    }

    pub(crate) async fn set_selected_pair(&mut self, p: Option<Arc<CandidatePair>>) {
        log::trace!("Set selected candidate pair: {:?}", p);

        if let Some(p) = p {
            p.nominated.store(true, Ordering::SeqCst);
            {
                let mut selected_pair = self.agent_conn.selected_pair.lock().await;
                *selected_pair = Some(p);
            }

            self.update_connection_state(ConnectionState::Connected)
                .await;

            // Notify when the selected pair changes
            let _ = self.chan_candidate_pair_tx.send(()).await;

            // Signal connected
            self.on_connected_tx.take();
        } else {
            let mut selected_pair = self.agent_conn.selected_pair.lock().await;
            *selected_pair = None;
        }
    }

    pub(crate) async fn ping_all_candidates(&mut self) {
        log::trace!("pinging all candidates");

        let mut pairs: Vec<(
            Arc<dyn Candidate + Send + Sync>,
            Arc<dyn Candidate + Send + Sync>,
        )> = vec![];

        {
            let mut checklist = self.agent_conn.checklist.lock().await;
            if checklist.is_empty() {
                log::warn!(
                    "pingAllCandidates called with no candidate pairs. Connection is not possible yet."
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
                    log::trace!("max requests reached for pair {}, marking it as failed", p);
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
        &mut self,
        local: Arc<dyn Candidate + Send + Sync>,
        remote: Arc<dyn Candidate + Send + Sync>,
    ) {
        let p = Arc::new(CandidatePair::new(local, remote, self.is_controlling));
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

    // validate_selected_pair checks if the selected pair is (still) valid
    // Note: the caller should hold the agent lock.
    pub(crate) async fn validate_selected_pair(&mut self) -> bool {
        let (valid, disconnected_time) = {
            let selected_pair = self.agent_conn.selected_pair.lock().await;
            if let Some(selected_pair) = &*selected_pair {
                let disconnected_time =
                    match SystemTime::now().duration_since(selected_pair.remote.last_received()) {
                        Ok(d) => d,
                        Err(_) => Duration::from_secs(0),
                    };
                (true, disconnected_time)
            } else {
                (false, Duration::from_secs(0))
            }
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

    // checkKeepalive sends STUN Binding Indications to the selected pair
    // if no packet has been sent on that pair in the last keepaliveInterval
    // Note: the caller should hold the agent lock.
    pub(crate) async fn check_keepalive(&mut self) {
        let (local, remote) = {
            let selected_pair = self.agent_conn.selected_pair.lock().await;
            if let Some(selected_pair) = &*selected_pair {
                (
                    Some(selected_pair.local.clone()),
                    Some(selected_pair.remote.clone()),
                )
            } else {
                (None, None)
            }
        };

        if let (Some(local), Some(remote)) = (local, remote) {
            let last_sent = match SystemTime::now().duration_since(local.last_sent()) {
                Ok(d) => d,
                Err(_) => Duration::from_secs(0),
            };

            let last_received = match SystemTime::now().duration_since(remote.last_received()) {
                Ok(d) => d,
                Err(_) => Duration::from_secs(0),
            };

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

    // add_remote_candidate assumes you are holding the lock (must be execute using a.run)
    pub(crate) async fn add_remote_candidate(&mut self, c: &Arc<dyn Candidate + Send + Sync>) {
        let network_type = c.network_type();

        if let Some(cands) = self.remote_candidates.get(&network_type) {
            for cand in cands {
                if cand.equal(&**c) {
                    return;
                }
            }
        }

        if let Some(cands) = self.remote_candidates.get_mut(&network_type) {
            cands.push(c.clone());
        } else {
            self.remote_candidates.insert(network_type, vec![c.clone()]);
        }

        let mut local_cands = vec![];
        if let Some(cands) = self.local_candidates.get(&network_type) {
            local_cands = cands.clone();
        }

        for cand in local_cands {
            self.add_pair(cand, c.clone()).await;
        }

        self.request_connectivity_check();
    }

    pub(crate) async fn add_candidate(
        &mut self,
        c: &Arc<dyn Candidate + Send + Sync>,
    ) -> Result<(), Error> {
        let initialized_ch = if let Some(started_ch_tx) = &self.started_ch_tx {
            Some(started_ch_tx.subscribe())
        } else {
            None
        };
        self.start_candidate(c, initialized_ch).await;

        let network_type = c.network_type();

        if let Some(cands) = self.local_candidates.get(&network_type) {
            for cand in cands {
                if cand.equal(&**c) {
                    if let Err(err) = c.close().await {
                        log::warn!("Failed to close duplicate candidate: {}", err);
                    }
                    //TODO: why return?
                    return Ok(());
                }
            }
        }

        if let Some(cands) = self.local_candidates.get_mut(&network_type) {
            cands.push(c.clone());
        } else {
            self.local_candidates.insert(network_type, vec![c.clone()]);
        }

        let mut remote_cands = vec![];
        if let Some(cands) = self.remote_candidates.get(&network_type) {
            remote_cands = cands.clone();
        }

        for cand in remote_cands {
            self.add_pair(c.clone(), cand).await;
        }

        self.request_connectivity_check();
        let _ = self.chan_candidate_tx.send(Some(c.clone())).await;

        Ok(())
    }

    pub(crate) fn close(&mut self) -> Result<(), Error> {
        if self.done_tx.is_none() {
            return Err(ERR_CLOSED.to_owned());
        }
        self.done_tx.take();
        self.agent_conn.done.store(true, Ordering::SeqCst);

        Ok(())
    }

    // Remove all candidates. This closes any listening sockets
    // and removes both the local and remote candidate lists.
    //
    // This is used for restarts, failures and on close
    pub(crate) async fn delete_all_candidates(&mut self) {
        for cs in &mut self.local_candidates.values_mut() {
            for c in cs {
                if let Err(err) = c.close().await {
                    log::warn!("Failed to close candidate {}: {}", c, err);
                }
            }
        }
        self.local_candidates.clear();

        for cs in self.remote_candidates.values_mut() {
            for c in cs {
                if let Err(err) = c.close().await {
                    log::warn!("Failed to close candidate {}: {}", c, err);
                }
            }
        }
        self.remote_candidates.clear();
    }

    pub(crate) fn find_remote_candidate(
        &self,
        network_type: NetworkType,
        addr: SocketAddr,
    ) -> Option<Arc<dyn Candidate + Send + Sync>> {
        let (ip, port) = (addr.ip(), addr.port());

        if let Some(cands) = self.remote_candidates.get(&network_type) {
            for c in cands {
                if c.address() == ip.to_string() && c.port() == port {
                    return Some(c.clone());
                }
            }
        }
        None
    }

    pub(crate) async fn send_binding_request(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        log::trace!("ping STUN from {} to {}", local, remote);

        self.invalidate_pending_binding_requests(Instant::now());
        self.pending_binding_requests.push(BindingRequest {
            timestamp: Instant::now(),
            transaction_id: m.transaction_id,
            destination: remote.addr().await,
            is_use_candidate: m.contains(ATTR_USE_CANDIDATE),
        });

        self.send_stun(m, local, remote).await;
    }

    pub(crate) async fn send_binding_success(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        let addr = remote.addr().await;
        let (ip, port) = (addr.ip(), addr.port());

        let (out, result) = {
            let mut out = Message::new();
            let result = out.build(&[
                Box::new(m.clone()),
                Box::new(BINDING_SUCCESS),
                Box::new(XORMappedAddress { ip, port }),
                Box::new(MessageIntegrity::new_short_term_integrity(
                    self.local_pwd.clone(),
                )),
                Box::new(FINGERPRINT),
            ]);
            (out, result)
        };

        if let Err(err) = result {
            log::warn!(
                "Failed to handle inbound ICE from: {} to: {} error: {}",
                local,
                remote,
                err
            );
        } else {
            self.send_stun(&out, local, remote).await;
        }
    }

    /* Removes pending binding requests that are over maxBindingRequestTimeout old
       Let HTO be the transaction timeout, which SHOULD be 2*RTT if
       RTT is known or 500 ms otherwise.
       https://tools.ietf.org/html/rfc8445#appendix-B.1
    */
    pub(crate) fn invalidate_pending_binding_requests(&mut self, filter_time: Instant) {
        let initial_size = self.pending_binding_requests.len();

        let mut temp = vec![];
        for binding_request in self.pending_binding_requests.drain(..) {
            if filter_time.duration_since(binding_request.timestamp) < MAX_BINDING_REQUEST_TIMEOUT {
                temp.push(binding_request);
            }
        }

        self.pending_binding_requests = temp;
        let bind_requests_removed = initial_size - self.pending_binding_requests.len();
        if bind_requests_removed > 0 {
            log::trace!(
                "Discarded {} binding requests because they expired",
                bind_requests_removed
            );
        }
    }

    // Assert that the passed TransactionID is in our pendingBindingRequests and returns the destination
    // If the bindingRequest was valid remove it from our pending cache
    pub(crate) fn handle_inbound_binding_success(
        &mut self,
        id: TransactionId,
    ) -> Option<BindingRequest> {
        self.invalidate_pending_binding_requests(Instant::now());
        for i in 0..self.pending_binding_requests.len() {
            if self.pending_binding_requests[i].transaction_id == id {
                let valid_binding_request = self.pending_binding_requests.remove(i);
                return Some(valid_binding_request);
            }
        }
        None
    }

    // handleInbound processes STUN traffic from a remote candidate
    pub(crate) async fn handle_inbound(
        &mut self,
        m: &mut Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: SocketAddr,
        agent_internal: Arc<Mutex<AgentInternal>>,
    ) {
        if m.typ.method != METHOD_BINDING
            || !(m.typ.class == CLASS_SUCCESS_RESPONSE
                || m.typ.class == CLASS_REQUEST
                || m.typ.class == CLASS_INDICATION)
        {
            log::trace!(
                "unhandled STUN from {} to {} class({}) method({})",
                remote,
                local,
                m.typ.class,
                m.typ.method
            );
            return;
        }

        if self.is_controlling {
            if m.contains(ATTR_ICE_CONTROLLING) {
                log::debug!("inbound isControlling && a.isControlling == true");
                return;
            } else if m.contains(ATTR_USE_CANDIDATE) {
                log::debug!("useCandidate && a.isControlling == true");
                return;
            }
        } else if m.contains(ATTR_ICE_CONTROLLED) {
            log::debug!("inbound isControlled && a.isControlling == false");
            return;
        }

        let mut remote_candidate = self.find_remote_candidate(local.network_type(), remote);
        if m.typ.class == CLASS_SUCCESS_RESPONSE {
            if let Err(err) = assert_inbound_message_integrity(m, self.remote_pwd.as_bytes()) {
                log::warn!("discard message from ({}), {}", remote, err);
                return;
            }

            if let Some(rc) = &remote_candidate {
                self.handle_success_response(m, local, rc, remote).await;
            } else {
                log::warn!("discard success message from ({}), no such remote", remote);
                return;
            }
        } else if m.typ.class == CLASS_REQUEST {
            let username = self.local_ufrag.clone() + ":" + self.remote_ufrag.as_str();
            if let Err(err) = assert_inbound_username(m, username) {
                log::warn!("discard message from ({}), {}", remote, err);
                return;
            } else if let Err(err) = assert_inbound_message_integrity(m, self.local_pwd.as_bytes())
            {
                log::warn!("discard message from ({}), {}", remote, err);
                return;
            }

            if remote_candidate.is_none() {
                let (ip, port, network_type) = (remote.ip(), remote.port(), NetworkType::Udp4);

                let prflx_candidate_config = CandidatePeerReflexiveConfig {
                    base_config: CandidateBaseConfig {
                        network: network_type.to_string(),
                        address: ip.to_string(),
                        port,
                        component: local.component(),
                        ..Default::default()
                    },
                    rel_addr: "".to_owned(),
                    rel_port: 0,
                };

                match prflx_candidate_config
                    .new_candidate_peer_reflexive(agent_internal)
                    .await
                {
                    Ok(prflx_candidate) => remote_candidate = Some(Arc::new(prflx_candidate)),
                    Err(err) => {
                        log::error!("Failed to create new remote prflx candidate ({})", err);
                        return;
                    }
                };

                log::debug!("adding a new peer-reflexive candidate: {} ", remote);
                if let Some(rc) = &remote_candidate {
                    self.add_remote_candidate(rc).await;
                }
            }

            log::trace!("inbound STUN (Request) from {} to {}", remote, local);

            if let Some(rc) = &remote_candidate {
                self.handle_binding_request(m, local, rc).await;
            }
        }

        if let Some(rc) = remote_candidate {
            rc.seen(false);
        }
    }

    // validate_non_stuntraffic processes non STUN traffic from a remote candidate,
    // and returns true if it is an actual remote candidate
    pub(crate) async fn validate_non_stun_traffic(
        &self,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: SocketAddr,
    ) -> bool {
        if let Some(remote_candidate) = self.find_remote_candidate(local.network_type(), remote) {
            remote_candidate.seen(false);
            true
        } else {
            false
        }
    }

    // set_remote_credentials sets the credentials of the remote agent
    pub(crate) fn set_remote_credentials(
        &mut self,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<(), Error> {
        if remote_ufrag.is_empty() {
            return Err(ERR_REMOTE_UFRAG_EMPTY.to_owned());
        } else if remote_pwd.is_empty() {
            return Err(ERR_REMOTE_PWD_EMPTY.to_owned());
        }

        self.remote_ufrag = remote_ufrag;
        self.remote_pwd = remote_pwd;
        Ok(())
    }

    pub(crate) async fn send_stun(
        &self,
        msg: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        if let Err(err) = local.write_to(&msg.raw, &**remote).await {
            log::trace!("failed to send STUN message: {}", err);
        }
    }

    // start runs the candidate using the provided connection
    async fn start_candidate(
        &self,
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
        if let (Some(conn), Some(ai)) = (candidate.get_conn(), candidate.get_agent()) {
            let conn = Arc::clone(conn);
            let addr = candidate.addr().await;
            let agent_internal = Arc::clone(ai);
            tokio::spawn(async move {
                let _ = CandidateBase::recv_loop(
                    cand,
                    agent_internal,
                    closed_ch_rx,
                    initialized_ch,
                    conn,
                    addr,
                )
                .await;
            });
        } else {
            log::error!("Can't start due to conn is_none");
        }
    }
}
