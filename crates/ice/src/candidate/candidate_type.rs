use std::fmt;

// CandidateType represents the type of candidate
// CandidateType enum
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum CandidateType {
    Unspecified,
    Host,
    ServerReflexive,
    PeerReflexive,
    Relay,
}

// String makes CandidateType printable
impl fmt::Display for CandidateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            CandidateType::Host => "host",
            CandidateType::ServerReflexive => "srflx",
            CandidateType::PeerReflexive => "prflx",
            CandidateType::Relay => "relay",
            CandidateType::Unspecified => "Unknown candidate type",
        };
        write!(f, "{}", s)
    }
}

impl Default for CandidateType {
    fn default() -> Self {
        CandidateType::Unspecified
    }
}

impl CandidateType {
    // preference returns the preference weight of a CandidateType
    //
    // 4.1.2.2.  Guidelines for Choosing Type and Local Preferences
    // The RECOMMENDED values are 126 for host candidates, 100
    // for server reflexive candidates, 110 for peer reflexive candidates,
    // and 0 for relayed candidates.
    pub fn preference(&self) -> u16 {
        match *self {
            CandidateType::Host => 126,
            CandidateType::PeerReflexive => 110,
            CandidateType::ServerReflexive => 100,
            CandidateType::Relay | CandidateType::Unspecified => 0,
        }
    }
}

pub(crate) fn contains_candidate_type(
    candidate_type: CandidateType,
    candidate_type_list: &[CandidateType],
) -> bool {
    if candidate_type_list.is_empty() {
        return false;
    }
    for ct in candidate_type_list {
        if *ct == candidate_type {
            return true;
        }
    }
    false
}
