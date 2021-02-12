use crate::candidate::*;

use crate::agent::AgentInternal;
use crate::candidate::candidate_pair::CandidatePair;
use crate::candidate::candidate_type::CandidateType;
use crate::control::*;
use crate::priority::*;
use crate::use_candidate::*;

use stun::{agent::*, attributes::*, fingerprint::*, integrity::*, message::*, textattrs::*};

use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Instant;

#[async_trait]
pub(crate) trait PairCandidateSelector {
    fn start(&mut self);
    async fn contact_candidates(&mut self);
    async fn ping_candidate(
        &self,
        local: &(dyn Candidate + Send + Sync),
        remote: &(dyn Candidate + Send + Sync),
    );
    fn handle_success_response(
        &self,
        m: &Message,
        local: &(dyn Candidate + Send + Sync),
        remote: &(dyn Candidate + Send + Sync),
        remote_addr: SocketAddr,
    );
    fn handle_binding_request(
        &self,
        m: &Message,
        local: &(dyn Candidate + Send + Sync),
        remote: &(dyn Candidate + Send + Sync),
    );
}

pub(crate) struct ControllingSelector {
    start_time: Instant,
    agent: Arc<Mutex<AgentInternal>>,
    nominated_pair: Option<CandidatePair>,
}

impl ControllingSelector {
    async fn is_nominatable(&self, c: &(dyn Candidate + Send + Sync)) -> bool {
        let agent = self.agent.lock().await;

        match c.candidate_type() {
            CandidateType::Host => {
                Instant::now().duration_since(self.start_time).as_nanos()
                    > agent.host_acceptance_min_wait.as_nanos()
            }
            CandidateType::ServerReflexive => {
                Instant::now().duration_since(self.start_time).as_nanos()
                    > agent.srflx_acceptance_min_wait.as_nanos()
            }
            CandidateType::PeerReflexive => {
                Instant::now().duration_since(self.start_time).as_nanos()
                    > agent.prflx_acceptance_min_wait.as_nanos()
            }
            CandidateType::Relay => {
                Instant::now().duration_since(self.start_time).as_nanos()
                    > agent.relay_acceptance_min_wait.as_nanos()
            }
            _ => {
                log::error!(
                    "is_nominatable invalid candidate type {}",
                    c.candidate_type()
                );
                false
            }
        }
    }

    async fn nominate_pair(&self, agent: &AgentInternal, pair: &CandidatePair) {
        // The controlling agent MUST include the USE-CANDIDATE attribute in
        // order to nominate a candidate pair (Section 8.1.1).  The controlled
        // agent MUST NOT include the USE-CANDIDATE attribute in a Binding
        // request.
        let mut msg = Message::new();
        if let Err(err) = msg.build(&[
            Box::new(BINDING_REQUEST),
            Box::new(TransactionId::default()),
            Box::new(Username::new(
                ATTR_USERNAME,
                agent.remote_ufrag.clone() + ":" + agent.local_ufrag.as_str(),
            )),
            Box::new(UseCandidateAttr::default()),
            Box::new(AttrControlling(agent.tie_breaker)),
            Box::new(PriorityAttr(pair.local.priority())),
            Box::new(MessageIntegrity::new_short_term_integrity(
                agent.remote_pwd.clone(),
            )),
            Box::new(FINGERPRINT),
        ]) {
            log::error!("{}", err);
            return;
        }

        log::trace!(
            "ping STUN (nominate candidate pair) from {} to {}",
            pair.local,
            pair.remote
        );
        //TODO: agent.send_binding_request(&msg, &*(pair.local), &*(pair.remote));
    }
}

#[async_trait]
impl PairCandidateSelector for ControllingSelector {
    fn start(&mut self) {
        self.start_time = Instant::now();
        self.nominated_pair = None;
    }

    async fn contact_candidates(&mut self) {
        let mut agent = self.agent.lock().await;

        if agent.get_selected_pair().is_some() {
            if agent.validate_selected_pair().await {
                log::trace!("checking keepalive");
                agent.check_keepalive().await;
            }
        } else if let Some(pair) = &self.nominated_pair {
            self.nominate_pair(&agent, pair).await;
        } else {
            let mut has_nominated_pair = false;
            if let Some(p) = agent.get_best_available_candidate_pair_mut() {
                if self.is_nominatable(&*(p.local)).await && self.is_nominatable(&*(p.remote)).await
                {
                    log::trace!(
                        "Nominatable pair found, nominating ({}, {})",
                        p.local.to_string(),
                        p.remote.to_string()
                    );
                    p.nominated = true;
                    self.nominated_pair = Some(p.clone());
                    has_nominated_pair = true;
                }
            }

            if has_nominated_pair {
                if let Some(pair) = &self.nominated_pair {
                    self.nominate_pair(&agent, pair).await;
                }
            } else {
                agent.ping_all_candidates().await;
            }
        }
    }

    async fn ping_candidate(
        &self,
        _local: &(dyn Candidate + Send + Sync),
        _remote: &(dyn Candidate + Send + Sync),
    ) {
    }

    fn handle_success_response(
        &self,
        _m: &Message,
        _local: &(dyn Candidate + Send + Sync),
        _remote: &(dyn Candidate + Send + Sync),
        _remote_addr: SocketAddr,
    ) {
    }
    fn handle_binding_request(
        &self,
        _m: &Message,
        _local: &(dyn Candidate + Send + Sync),
        _remote: &(dyn Candidate + Send + Sync),
    ) {
    }

    /*
    func (s *ControllingSelector) nominatePair(pair *candidatePair) {
        // The controlling agent MUST include the USE-CANDIDATE attribute in
        // order to nominate a candidate pair (Section 8.1.1).  The controlled
        // agent MUST NOT include the USE-CANDIDATE attribute in a Binding
        // request.
        msg, err := stun.Build(stun.BindingRequest, stun.TransactionID,
            stun.NewUsername(s.agent.remoteUfrag+":"+s.agent.localUfrag),
            UseCandidate(),
            AttrControlling(s.agent.tieBreaker),
            PriorityAttr(pair.local.Priority()),
            stun.NewShortTermIntegrity(s.agent.remotePwd),
            stun.Fingerprint,
        )
        if err != nil {
            s.log.Error(err.Error())
            return
        }

        s.log.Tracef("ping STUN (nominate candidate pair) from %s to %s\n", pair.local.String(), pair.remote.String())
        s.agent.sendBindingRequest(msg, pair.local, pair.remote)
    }

    func (s *ControllingSelector) handle_binding_request(m *stun.Message, local, remote Candidate) {
        s.agent.sendBindingSuccess(m, local, remote)

        p := s.agent.findPair(local, remote)

        if p == nil {
            s.agent.addPair(local, remote)
            return
        }

        if p.state == CandidatePairStateSucceeded && s.nominated_pair == nil && s.agent.getSelectedPair() == nil {
            bestPair := s.agent.getBestAvailableCandidatePair()
            if bestPair == nil {
                s.log.Tracef("No best pair available\n")
            } else if bestPair.Equal(p) && s.is_nominatable(p.local) && s.is_nominatable(p.remote) {
                s.log.Tracef("The candidate (%s, %s) is the best candidate available, marking it as nominated\n",
                    p.local.String(), p.remote.String())
                s.nominated_pair = p
                s.nominatePair(p)
            }
        }
    }

    func (s *ControllingSelector) handle_success_response(m *stun.Message, local, remote Candidate, remoteAddr net.Addr) {
        ok, pendingRequest := s.agent.handleInboundBindingSuccess(m.TransactionID)
        if !ok {
            s.log.Warnf("discard message from (%s), unknown TransactionID 0x%x", remote, m.TransactionID)
            return
        }

        transactionAddr := pendingRequest.destination

        // Assert that NAT is not symmetric
        // https://tools.ietf.org/html/rfc8445#section-7.2.5.2.1
        if !addrEqual(transactionAddr, remoteAddr) {
            s.log.Debugf("discard message: transaction source and destination does not match expected(%s), actual(%s)", transactionAddr, remote)
            return
        }

        s.log.Tracef("inbound STUN (SuccessResponse) from %s to %s", remote.String(), local.String())
        p := s.agent.findPair(local, remote)

        if p == nil {
            // This shouldn't happen
            s.log.Error("Success response from invalid candidate pair")
            return
        }

        p.state = CandidatePairStateSucceeded
        s.log.Tracef("Found valid candidate pair: %s", p)
        if pendingRequest.isUseCandidate && s.agent.getSelectedPair() == nil {
            s.agent.setSelectedPair(p)
        }
    }

    func (s *ControllingSelector) ping_candidate(local, remote Candidate) {
        msg, err := stun.Build(stun.BindingRequest, stun.TransactionID,
            stun.NewUsername(s.agent.remoteUfrag+":"+s.agent.localUfrag),
            AttrControlling(s.agent.tieBreaker),
            PriorityAttr(local.Priority()),
            stun.NewShortTermIntegrity(s.agent.remotePwd),
            stun.Fingerprint,
        )
        if err != nil {
            s.log.Error(err.Error())
            return
        }

        s.agent.sendBindingRequest(msg, local, remote)
    }*/
}

/*
type controlledSelector struct {
    agent *Agent
    log   logging.LeveledLogger
}

func (s *controlledSelector) Start() {
}

func (s *controlledSelector) contact_candidates() {
    if s.agent.getSelectedPair() != nil {
        if s.agent.validate_selected_pair() {
            s.log.Trace("checking keepalive")
            s.agent.checkKeepalive()
        }
    } else {
        s.agent.pingAllCandidates()
    }
}

func (s *controlledSelector) ping_candidate(local, remote Candidate) {
    msg, err := stun.Build(stun.BindingRequest, stun.TransactionID,
        stun.NewUsername(s.agent.remoteUfrag+":"+s.agent.localUfrag),
        AttrControlled(s.agent.tieBreaker),
        PriorityAttr(local.Priority()),
        stun.NewShortTermIntegrity(s.agent.remotePwd),
        stun.Fingerprint,
    )
    if err != nil {
        s.log.Error(err.Error())
        return
    }

    s.agent.sendBindingRequest(msg, local, remote)
}

func (s *controlledSelector) handle_success_response(m *stun.Message, local, remote Candidate, remoteAddr net.Addr) {
    // nolint:godox
    // TODO according to the standard we should specifically answer a failed nomination:
    // https://tools.ietf.org/html/rfc8445#section-7.3.1.5
    // If the controlled agent does not accept the request from the
    // controlling agent, the controlled agent MUST reject the nomination
    // request with an appropriate error code response (e.g., 400)
    // [RFC5389].

    ok, pendingRequest := s.agent.handleInboundBindingSuccess(m.TransactionID)
    if !ok {
        s.log.Warnf("discard message from (%s), unknown TransactionID 0x%x", remote, m.TransactionID)
        return
    }

    transactionAddr := pendingRequest.destination

    // Assert that NAT is not symmetric
    // https://tools.ietf.org/html/rfc8445#section-7.2.5.2.1
    if !addrEqual(transactionAddr, remoteAddr) {
        s.log.Debugf("discard message: transaction source and destination does not match expected(%s), actual(%s)", transactionAddr, remote)
        return
    }

    s.log.Tracef("inbound STUN (SuccessResponse) from %s to %s", remote.String(), local.String())

    p := s.agent.findPair(local, remote)
    if p == nil {
        // This shouldn't happen
        s.log.Error("Success response from invalid candidate pair")
        return
    }

    p.state = CandidatePairStateSucceeded
    s.log.Tracef("Found valid candidate pair: %s", p)
}

func (s *controlledSelector) handle_binding_request(m *stun.Message, local, remote Candidate) {
    useCandidate := m.Contains(stun.AttrUseCandidate)

    p := s.agent.findPair(local, remote)

    if p == nil {
        p = s.agent.addPair(local, remote)
    }

    if useCandidate {
        // https://tools.ietf.org/html/rfc8445#section-7.3.1.5

        if p.state == CandidatePairStateSucceeded {
            // If the state of this pair is Succeeded, it means that the check
            // previously sent by this pair produced a successful response and
            // generated a valid pair (Section 7.2.5.3.2).  The agent sets the
            // nominated flag value of the valid pair to true.
            if selected_pair := s.agent.getSelectedPair(); selected_pair == nil {
                s.agent.setSelectedPair(p)
            }
            s.agent.sendBindingSuccess(m, local, remote)
        } else {
            // If the received Binding request triggered a new check to be
            // enqueued in the triggered-check queue (Section 7.3.1.4), once the
            // check is sent and if it generates a successful response, and
            // generates a valid pair, the agent sets the nominated flag of the
            // pair to true.  If the request fails (Section 7.2.5.2), the agent
            // MUST remove the candidate pair from the valid list, set the
            // candidate pair state to Failed, and set the checklist state to
            // Failed.
            s.ping_candidate(local, remote)
        }
    } else {
        s.agent.sendBindingSuccess(m, local, remote)
        s.ping_candidate(local, remote)
    }
}

type liteSelector struct {
    PairCandidateSelector
}

// A lite selector should not contact candidates
func (s *liteSelector) contact_candidates() {
    if _, ok := s.PairCandidateSelector.(*ControllingSelector); ok {
        // nolint:godox
        // pion/ice#96
        // TODO: implement lite controlling agent. For now falling back to full agent.
        // This only happens if both peers are lite. See RFC 8445 S6.1.1 and S6.2
        s.PairCandidateSelector.contact_candidates()
    } else if v, ok := s.PairCandidateSelector.(*controlledSelector); ok {
        v.agent.validate_selected_pair()
    }
}
*/
