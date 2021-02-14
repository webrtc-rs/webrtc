use crate::candidate::*;

use crate::agent::AgentInternal;
use crate::candidate::candidate_pair::CandidatePairState;
use crate::candidate::candidate_type::CandidateType;
use crate::control::*;
use crate::priority::*;
use crate::use_candidate::*;

use stun::{agent::*, attributes::*, fingerprint::*, integrity::*, message::*, textattrs::*};

use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::Instant;

#[async_trait]
trait ControllingSelector {
    fn start(&mut self);
    async fn contact_candidates(&mut self);
    async fn ping_candidate(
        &mut self,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    );
    async fn handle_success_response(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
        remote_addr: SocketAddr,
    );
    async fn handle_binding_request(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    );
}

#[async_trait]
trait ControlledSelector {
    fn start(&mut self);
    async fn contact_candidates(&mut self);
    async fn ping_candidate(
        &mut self,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    );
    async fn handle_success_response(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
        remote_addr: SocketAddr,
    );
    async fn handle_binding_request(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    );
}

impl AgentInternal {
    async fn is_nominatable(&self, c: &Arc<dyn Candidate + Send + Sync>) -> bool {
        match c.candidate_type() {
            CandidateType::Host => {
                Instant::now().duration_since(self.start_time).as_nanos()
                    > self.host_acceptance_min_wait.as_nanos()
            }
            CandidateType::ServerReflexive => {
                Instant::now().duration_since(self.start_time).as_nanos()
                    > self.srflx_acceptance_min_wait.as_nanos()
            }
            CandidateType::PeerReflexive => {
                Instant::now().duration_since(self.start_time).as_nanos()
                    > self.prflx_acceptance_min_wait.as_nanos()
            }
            CandidateType::Relay => {
                Instant::now().duration_since(self.start_time).as_nanos()
                    > self.relay_acceptance_min_wait.as_nanos()
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

    async fn nominate_pair(&mut self) {
        if let Some(pair) = &self.nominated_pair {
            // The controlling agent MUST include the USE-CANDIDATE attribute in
            // order to nominate a candidate pair (Section 8.1.1).  The controlled
            // agent MUST NOT include the USE-CANDIDATE attribute in a Binding
            // request.
            let username = self.remote_ufrag.clone() + ":" + self.local_ufrag.as_str();
            let mut msg = Message::new();
            if let Err(err) = msg.build(&[
                Box::new(BINDING_REQUEST),
                Box::new(TransactionId::default()),
                Box::new(Username::new(ATTR_USERNAME, username)),
                Box::new(UseCandidateAttr::default()),
                Box::new(AttrControlling(self.tie_breaker)),
                Box::new(PriorityAttr(pair.local.priority())),
                Box::new(MessageIntegrity::new_short_term_integrity(
                    self.remote_pwd.clone(),
                )),
                Box::new(FINGERPRINT),
            ]) {
                log::error!("{}", err);
            } else {
                log::trace!(
                    "ping STUN (nominate candidate pair from {} to {}",
                    pair.local,
                    pair.remote
                );
                //TODO: self.send_binding_request(&msg, &*(pair.local), &*(pair.remote));
            }
        }
    }

    pub(crate) fn start(&mut self) {
        if self.is_controlling {
            ControllingSelector::start(self);
        } else {
            ControlledSelector::start(self);
        }
    }

    pub(crate) async fn contact_candidates(&mut self) {
        if self.is_controlling {
            ControllingSelector::contact_candidates(self).await;
        } else {
            ControlledSelector::contact_candidates(self).await;
        }
    }

    pub(crate) async fn ping_candidate(
        &mut self,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        if self.is_controlling {
            ControllingSelector::ping_candidate(self, local, remote).await;
        } else {
            ControlledSelector::ping_candidate(self, local, remote).await;
        }
    }

    pub(crate) async fn handle_success_response(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
        remote_addr: SocketAddr,
    ) {
        if self.is_controlling {
            ControllingSelector::handle_success_response(self, m, local, remote, remote_addr).await;
        } else {
            ControlledSelector::handle_success_response(self, m, local, remote, remote_addr).await;
        }
    }

    pub(crate) async fn handle_binding_request(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        if self.is_controlling {
            ControllingSelector::handle_binding_request(self, m, local, remote).await;
        } else {
            ControlledSelector::handle_binding_request(self, m, local, remote).await;
        }
    }
}

#[async_trait]
impl ControllingSelector for AgentInternal {
    fn start(&mut self) {
        self.start_time = Instant::now();
        self.nominated_pair = None;
    }

    async fn contact_candidates(&mut self) {
        // A lite selector should not contact candidates
        if self.lite {
            // TODO: implement lite controlling agent. For now falling back to full agent.
            // This only happens if both peers are lite. See RFC 8445 S6.1.1 and S6.2
            log::trace!("now falling back to full agent");
        }

        if self.get_selected_pair().is_some() {
            if self.validate_selected_pair().await {
                log::trace!("checking keepalive");
                self.check_keepalive().await;
            }
        } else if self.nominated_pair.is_some() {
            self.nominate_pair().await;
        } else {
            let has_nominated_pair = if let Some(p) = self.get_best_available_candidate_pair() {
                self.is_nominatable(&p.local).await && self.is_nominatable(&p.remote).await
            } else {
                false
            };

            if has_nominated_pair {
                if let Some(p) = self.get_best_available_candidate_pair_mut() {
                    log::trace!(
                        "Nominatable pair found, nominating ({}, {})",
                        p.local.to_string(),
                        p.remote.to_string()
                    );
                    p.nominated = true;
                    self.nominated_pair = Some(p.clone());
                }

                self.nominate_pair().await;
            } else {
                self.ping_all_candidates().await;
            }
        }
    }

    async fn ping_candidate(
        &mut self,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        let username = self.remote_ufrag.clone() + ":" + self.local_ufrag.as_str();
        let mut msg = Message::new();
        if let Err(err) = msg.build(&[
            Box::new(BINDING_REQUEST),
            Box::new(TransactionId::default()),
            Box::new(Username::new(ATTR_USERNAME, username)),
            Box::new(AttrControlling(self.tie_breaker)),
            Box::new(PriorityAttr(local.priority())),
            Box::new(MessageIntegrity::new_short_term_integrity(
                self.remote_pwd.clone(),
            )),
            Box::new(FINGERPRINT),
        ]) {
            log::error!("{}", err);
        } else {
            self.send_binding_request(&msg, local, remote);
        }
    }

    async fn handle_success_response(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
        remote_addr: SocketAddr,
    ) {
        if let Some(pending_request) = self.handle_inbound_binding_success(m.transaction_id) {
            let transaction_addr = pending_request.destination;

            // Assert that NAT is not symmetric
            // https://tools.ietf.org/html/rfc8445#section-7.2.5.2.1
            if transaction_addr != remote_addr {
                log::debug!("discard message: transaction source and destination does not match expected({}), actual({})", transaction_addr, remote);
                return;
            }

            log::trace!(
                "inbound STUN (SuccessResponse) from {} to {}",
                remote,
                local
            );
            let selected_pair_is_none = self.get_selected_pair().is_none();

            if let Some(p) = self.find_pair(local, remote) {
                let mut p = p.clone();
                p.state = CandidatePairState::Succeeded;
                log::trace!("Found valid candidate pair: {}", p);
                if pending_request.is_use_candidate && selected_pair_is_none {
                    self.set_selected_pair(Some(p.clone())).await;
                }
            } else {
                // This shouldn't happen
                log::error!("Success response from invalid candidate pair");
            }
        } else {
            log::warn!(
                "discard message from ({}), unknown TransactionID 0x{:?}",
                remote,
                m.transaction_id
            );
        }
    }

    async fn handle_binding_request(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        self.send_binding_request(m, local, remote);

        if let Some(p) = self.find_pair(local, remote) {
            if p.state == CandidatePairState::Succeeded
                && self.nominated_pair.is_none()
                && self.get_selected_pair().is_none()
            {
                if let Some(best_pair) = self.get_best_available_candidate_pair() {
                    if best_pair == p
                        && self.is_nominatable(&p.local).await
                        && self.is_nominatable(&p.remote).await
                    {
                        log::trace!("The candidate ({}, {}) is the best candidate available, marking it as nominated",
                            p.local, p.remote);
                        self.nominated_pair = Some(p.clone());
                        self.nominate_pair().await;
                    }
                } else {
                    log::trace!("No best pair available");
                }
            }
        } else {
            self.add_pair(local.clone(), remote.clone());
        }
    }
}

#[async_trait]
impl ControlledSelector for AgentInternal {
    fn start(&mut self) {}

    async fn contact_candidates(&mut self) {
        // A lite selector should not contact candidates
        if self.lite {
            self.validate_selected_pair().await;
        } else if self.get_selected_pair().is_some() {
            if self.validate_selected_pair().await {
                log::trace!("checking keepalive");
                self.check_keepalive().await;
            }
        } else {
            self.ping_all_candidates().await;
        }
    }

    async fn ping_candidate(
        &mut self,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        let username = self.remote_ufrag.clone() + ":" + self.local_ufrag.as_str();
        let mut msg = Message::new();
        if let Err(err) = msg.build(&[
            Box::new(BINDING_REQUEST),
            Box::new(TransactionId::default()),
            Box::new(Username::new(ATTR_USERNAME, username)),
            Box::new(AttrControlled(self.tie_breaker)),
            Box::new(PriorityAttr(local.priority())),
            Box::new(MessageIntegrity::new_short_term_integrity(
                self.remote_pwd.clone(),
            )),
            Box::new(FINGERPRINT),
        ]) {
            log::error!("{}", err);
        } else {
            self.send_binding_request(&msg, local, remote);
        }
    }

    async fn handle_success_response(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
        remote_addr: SocketAddr,
    ) {
        // TODO according to the standard we should specifically answer a failed nomination:
        // https://tools.ietf.org/html/rfc8445#section-7.3.1.5
        // If the controlled agent does not accept the request from the
        // controlling agent, the controlled agent MUST reject the nomination
        // request with an appropriate error code response (e.g., 400)
        // [RFC5389].

        if let Some(pending_request) = self.handle_inbound_binding_success(m.transaction_id) {
            let transaction_addr = pending_request.destination;

            // Assert that NAT is not symmetric
            // https://tools.ietf.org/html/rfc8445#section-7.2.5.2.1
            if transaction_addr != remote_addr {
                log::debug!("discard message: transaction source and destination does not match expected({}), actual({})", transaction_addr, remote);
                return;
            }

            log::trace!(
                "inbound STUN (SuccessResponse) from {} to {}",
                remote,
                local
            );

            if let Some(p) = self.get_pair_mut(local, remote) {
                p.state = CandidatePairState::Succeeded;
                log::trace!("Found valid candidate pair: {}", p);
            } else {
                // This shouldn't happen
                log::error!("Success response from invalid candidate pair");
            }
        } else {
            log::warn!(
                "discard message from ({}), unknown TransactionID 0x{:?}",
                remote,
                m.transaction_id
            );
        }
    }

    async fn handle_binding_request(
        &mut self,
        m: &Message,
        local: &Arc<dyn Candidate + Send + Sync>,
        remote: &Arc<dyn Candidate + Send + Sync>,
    ) {
        if self.find_pair(local, remote).is_none() {
            self.add_pair(local.clone(), remote.clone());
        }

        if let Some(p) = self.find_pair(local, remote) {
            let use_candidate = m.contains(ATTR_USE_CANDIDATE);
            if use_candidate {
                // https://tools.ietf.org/html/rfc8445#section-7.3.1.5

                if p.state == CandidatePairState::Succeeded {
                    // If the state of this pair is Succeeded, it means that the check
                    // previously sent by this pair produced a successful response and
                    // generated a valid pair (Section 7.2.5.3.2).  The agent sets the
                    // nominated flag value of the valid pair to true.
                    if self.get_selected_pair().is_none() {
                        let pair = p.clone();
                        self.set_selected_pair(Some(pair)).await;
                    }
                    self.send_binding_success(m, local, remote);
                } else {
                    // If the received Binding request triggered a new check to be
                    // enqueued in the triggered-check queue (Section 7.3.1.4), once the
                    // check is sent and if it generates a successful response, and
                    // generates a valid pair, the agent sets the nominated flag of the
                    // pair to true.  If the request fails (Section 7.2.5.2), the agent
                    // MUST remove the candidate pair from the valid list, set the
                    // candidate pair state to Failed, and set the checklist state to
                    // Failed.
                    self.ping_candidate(local, remote).await;
                }
            } else {
                self.send_binding_success(m, local, remote);
                self.ping_candidate(local, remote).await;
            }
        }
    }
}
