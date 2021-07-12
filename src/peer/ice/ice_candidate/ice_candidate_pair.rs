use super::*;

/// ICECandidatePair represents an ICE Candidate pair
#[derive(Default, Debug, Clone, PartialEq)]
pub struct ICECandidatePair {
    stats_id: String,
    local: ICECandidate,
    remote: ICECandidate,
}

impl fmt::Display for ICECandidatePair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(local) {} <-> (remote) {}", self.local, self.remote)
    }
}

impl ICECandidatePair {
    fn stats_id(local_id: &str, remote_id: &str) -> String {
        format!("{}-{}", local_id, remote_id)
    }

    /// returns an initialized ICECandidatePair
    /// for the given pair of ICECandidate instances
    pub fn new(local: ICECandidate, remote: ICECandidate) -> Self {
        let stats_id = Self::stats_id(&local.stats_id, &remote.stats_id);
        ICECandidatePair {
            stats_id,
            local,
            remote,
        }
    }
}
