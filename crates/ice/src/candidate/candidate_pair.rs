use super::*;

use std::cmp;
use std::fmt;

// CandidatePairState represent the ICE candidate pair state
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum CandidatePairState {
    // CandidatePairStateWaiting means a check has not been performed for
    // this pair
    Waiting,

    // CandidatePairStateInProgress means a check has been sent for this pair,
    // but the transaction is in progress.
    InProgress,

    // CandidatePairStateFailed means a check for this pair was already done
    // and failed, either never producing any response or producing an unrecoverable
    // failure response.
    Failed,

    // CandidatePairStateSucceeded means a check for this pair was already
    // done and produced a successful result.
    Succeeded,
}

impl fmt::Display for CandidatePairState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            CandidatePairState::Waiting => "waiting",
            CandidatePairState::InProgress => "in-progress",
            CandidatePairState::Failed => "failed",
            CandidatePairState::Succeeded => "succeeded",
        };

        write!(f, "{}", s)
    }
}

// candidatePair represents a combination of a local and remote candidate
pub struct CandidatePair {
    ice_role_controlling: bool,
    remote: Box<dyn Candidate>,
    local: Box<dyn Candidate>,
    binding_request_count: u16,
    state: CandidatePairState,
    nominated: bool,
}

impl fmt::Display for CandidatePair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "prio {} (local, prio {}) {} <-> {} (remote, prio {})",
            self.priority(),
            self.local.priority(),
            self.local,
            self.remote,
            self.remote.priority()
        )
    }
}

impl PartialEq for CandidatePair {
    fn eq(&self, other: &CandidatePair) -> bool {
        self.local.equal(&*other.local) && self.remote.equal(&*other.remote)
    }
}

impl CandidatePair {
    pub fn new(local: Box<dyn Candidate>, remote: Box<dyn Candidate>, controlling: bool) -> Self {
        CandidatePair {
            ice_role_controlling: controlling,
            remote,
            local,
            state: CandidatePairState::Waiting,
            binding_request_count: 0,
            nominated: false,
        }
    }

    // RFC 5245 - 5.7.2.  Computing Pair Priority and Ordering Pairs
    // Let G be the priority for the candidate provided by the controlling
    // agent.  Let D be the priority for the candidate provided by the
    // controlled agent.
    // pair priority = 2^32*MIN(G,D) + 2*MAX(G,D) + (G>D?1:0)
    pub fn priority(&self) -> u64 {
        let (g, d) = if self.ice_role_controlling {
            (self.local.priority(), self.remote.priority())
        } else {
            (self.remote.priority(), self.local.priority())
        };

        // 1<<32 overflows uint32; and if both g && d are
        // maxUint32, this result would overflow uint64
        ((1 << 32u64) - 1) * cmp::min(g, d) as u64
            + 2 * cmp::max(g, d) as u64
            + if g > d { 1 } else { 0 }
    }

    pub fn write(&mut self, b: &[u8]) -> Result<usize, Error> {
        self.local.write_to(b, &*self.remote)
    }
}

/*
func (a *Agent) sendSTUN(msg *stun.Message, local, remote Candidate) {
    _, err := local.writeTo(msg.Raw, remote)
    if err != nil {
        a.log.Tracef("failed to send STUN message: %s", err)
    }
}*/
