use crate::error::Error;
use crate::sdp::sdp_type::SDPType;
use std::fmt;

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum StateChangeOp {
    SetLocal,
    SetRemote,
}

impl Default for StateChangeOp {
    fn default() -> Self {
        StateChangeOp::SetLocal
    }
}

impl fmt::Display for StateChangeOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            StateChangeOp::SetLocal => write!(f, "SetLocal"),
            StateChangeOp::SetRemote => write!(f, "SetRemote"),
            //_ => write!(f, "Unspecified"),
        }
    }
}

/// SignalingState indicates the signaling state of the offer/answer process.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SignalingState {
    Unspecified = 0,

    /// SignalingStateStable indicates there is no offer/answer exchange in
    /// progress. This is also the initial state, in which case the local and
    /// remote descriptions are nil.
    Stable,

    /// SignalingStateHaveLocalOffer indicates that a local description, of
    /// type "offer", has been successfully applied.
    HaveLocalOffer,

    /// SignalingStateHaveRemoteOffer indicates that a remote description, of
    /// type "offer", has been successfully applied.
    HaveRemoteOffer,

    /// SignalingStateHaveLocalPranswer indicates that a remote description
    /// of type "offer" has been successfully applied and a local description
    /// of type "pranswer" has been successfully applied.
    HaveLocalPranswer,

    /// SignalingStateHaveRemotePranswer indicates that a local description
    /// of type "offer" has been successfully applied and a remote description
    /// of type "pranswer" has been successfully applied.
    HaveRemotePranswer,

    /// SignalingStateClosed indicates The PeerConnection has been closed.
    Closed,
}

impl Default for SignalingState {
    fn default() -> Self {
        SignalingState::Unspecified
    }
}

const SIGNALING_STATE_STABLE_STR: &str = "Stable";
const SIGNALING_STATE_HAVE_LOCAL_OFFER_STR: &str = "HaveLocalOffer";
const SIGNALING_STATE_HAVE_REMOTE_OFFER_STR: &str = "HaveRemoteOffer";
const SIGNALING_STATE_HAVE_LOCAL_PRANSWER_STR: &str = "HaveLocalPranswer";
const SIGNALING_STATE_HAVE_REMOTE_PRANSWER_STR: &str = "HaveRemotePranswer";
const SIGNALING_STATE_CLOSED_STR: &str = "Closed";

impl From<&str> for SignalingState {
    fn from(raw: &str) -> Self {
        match raw {
            SIGNALING_STATE_STABLE_STR => SignalingState::Stable,
            SIGNALING_STATE_HAVE_LOCAL_OFFER_STR => SignalingState::HaveLocalOffer,
            SIGNALING_STATE_HAVE_REMOTE_OFFER_STR => SignalingState::HaveRemoteOffer,
            SIGNALING_STATE_HAVE_LOCAL_PRANSWER_STR => SignalingState::HaveLocalPranswer,
            SIGNALING_STATE_HAVE_REMOTE_PRANSWER_STR => SignalingState::HaveRemotePranswer,
            SIGNALING_STATE_CLOSED_STR => SignalingState::Closed,
            _ => SignalingState::Unspecified,
        }
    }
}

impl fmt::Display for SignalingState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            SignalingState::Stable => write!(f, "{}", SIGNALING_STATE_STABLE_STR),
            SignalingState::HaveLocalOffer => write!(f, "{}", SIGNALING_STATE_HAVE_LOCAL_OFFER_STR),
            SignalingState::HaveRemoteOffer => {
                write!(f, "{}", SIGNALING_STATE_HAVE_REMOTE_OFFER_STR)
            }
            SignalingState::HaveLocalPranswer => {
                write!(f, "{}", SIGNALING_STATE_HAVE_LOCAL_PRANSWER_STR)
            }
            SignalingState::HaveRemotePranswer => {
                write!(f, "{}", SIGNALING_STATE_HAVE_REMOTE_PRANSWER_STR)
            }
            SignalingState::Closed => write!(f, "{}", SIGNALING_STATE_CLOSED_STR),
            _ => write!(f, "Unspecified"),
        }
    }
}

impl From<u8> for SignalingState {
    fn from(v: u8) -> Self {
        match v {
            1 => SignalingState::Stable,
            2 => SignalingState::HaveLocalOffer,
            3 => SignalingState::HaveRemoteOffer,
            4 => SignalingState::HaveLocalPranswer,
            5 => SignalingState::HaveRemotePranswer,
            6 => SignalingState::Closed,
            _ => SignalingState::Unspecified,
        }
    }
}

pub(crate) fn check_next_signaling_state(
    cur: SignalingState,
    next: SignalingState,
    op: StateChangeOp,
    sdp_type: SDPType,
) -> Result<SignalingState, Error> {
    // Special case for rollbacks
    if sdp_type == SDPType::Rollback && cur == SignalingState::Stable {
        return Err(Error::ErrSignalingStateCannotRollback);
    }

    // 4.3.1 valid state transitions
    match cur {
        SignalingState::Stable => {
            match op {
                StateChangeOp::SetLocal => {
                    // stable->SetLocal(offer)->have-local-offer
                    if sdp_type == SDPType::Offer && next == SignalingState::HaveLocalOffer {
                        return Ok(next);
                    }
                }
                StateChangeOp::SetRemote => {
                    // stable->SetRemote(offer)->have-remote-offer
                    if sdp_type == SDPType::Offer && next == SignalingState::HaveRemoteOffer {
                        return Ok(next);
                    }
                }
            }
        }
        SignalingState::HaveLocalOffer => {
            if op == StateChangeOp::SetRemote {
                match sdp_type {
                    // have-local-offer->SetRemote(answer)->stable
                    SDPType::Answer => {
                        if next == SignalingState::Stable {
                            return Ok(next);
                        }
                    }
                    // have-local-offer->SetRemote(pranswer)->have-remote-pranswer
                    SDPType::Pranswer => {
                        if next == SignalingState::HaveRemotePranswer {
                            return Ok(next);
                        }
                    }
                    _ => {}
                }
            }
        }
        SignalingState::HaveRemotePranswer => {
            if op == StateChangeOp::SetRemote && sdp_type == SDPType::Answer {
                // have-remote-pranswer->SetRemote(answer)->stable
                if next == SignalingState::Stable {
                    return Ok(next);
                }
            }
        }
        SignalingState::HaveRemoteOffer => {
            if op == StateChangeOp::SetLocal {
                match sdp_type {
                    // have-remote-offer->SetLocal(answer)->stable
                    SDPType::Answer => {
                        if next == SignalingState::Stable {
                            return Ok(next);
                        }
                    }
                    // have-remote-offer->SetLocal(pranswer)->have-local-pranswer
                    SDPType::Pranswer => {
                        if next == SignalingState::HaveLocalPranswer {
                            return Ok(next);
                        }
                    }
                    _ => {}
                }
            }
        }
        SignalingState::HaveLocalPranswer => {
            if op == StateChangeOp::SetLocal && sdp_type == SDPType::Answer {
                // have-local-pranswer->SetLocal(answer)->stable
                if next == SignalingState::Stable {
                    return Ok(next);
                }
            }
        }
        _ => {
            return Err(Error::ErrSignalingStateProposedTransitionInvalid);
        }
    };

    Err(Error::ErrSignalingStateProposedTransitionInvalid)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_signaling_state() {
        let tests = vec![
            ("Unspecified", SignalingState::Unspecified),
            ("Stable", SignalingState::Stable),
            ("HaveLocalOffer", SignalingState::HaveLocalOffer),
            ("HaveRemoteOffer", SignalingState::HaveRemoteOffer),
            ("HaveLocalPranswer", SignalingState::HaveLocalPranswer),
            ("HaveRemotePranswer", SignalingState::HaveRemotePranswer),
            ("Closed", SignalingState::Closed),
        ];

        for (state_string, expected_state) in tests {
            assert_eq!(expected_state, SignalingState::from(state_string));
        }
    }

    #[test]
    fn test_signaling_state_string() {
        let tests = vec![
            (SignalingState::Unspecified, "Unspecified"),
            (SignalingState::Stable, "Stable"),
            (SignalingState::HaveLocalOffer, "HaveLocalOffer"),
            (SignalingState::HaveRemoteOffer, "HaveRemoteOffer"),
            (SignalingState::HaveLocalPranswer, "HaveLocalPranswer"),
            (SignalingState::HaveRemotePranswer, "HaveRemotePranswer"),
            (SignalingState::Closed, "Closed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(expected_string, state.to_string());
        }
    }

    #[test]
    fn test_signaling_state_transitions() {
        let tests = vec![
            (
                "stable->SetLocal(offer)->have-local-offer",
                SignalingState::Stable,
                SignalingState::HaveLocalOffer,
                StateChangeOp::SetLocal,
                SDPType::Offer,
                None,
            ),
            (
                "stable->SetRemote(offer)->have-remote-offer",
                SignalingState::Stable,
                SignalingState::HaveRemoteOffer,
                StateChangeOp::SetRemote,
                SDPType::Offer,
                None,
            ),
            (
                "have-local-offer->SetRemote(answer)->stable",
                SignalingState::HaveLocalOffer,
                SignalingState::Stable,
                StateChangeOp::SetRemote,
                SDPType::Answer,
                None,
            ),
            (
                "have-local-offer->SetRemote(pranswer)->have-remote-pranswer",
                SignalingState::HaveLocalOffer,
                SignalingState::HaveRemotePranswer,
                StateChangeOp::SetRemote,
                SDPType::Pranswer,
                None,
            ),
            (
                "have-remote-pranswer->SetRemote(answer)->stable",
                SignalingState::HaveRemotePranswer,
                SignalingState::Stable,
                StateChangeOp::SetRemote,
                SDPType::Answer,
                None,
            ),
            (
                "have-remote-offer->SetLocal(answer)->stable",
                SignalingState::HaveRemoteOffer,
                SignalingState::Stable,
                StateChangeOp::SetLocal,
                SDPType::Answer,
                None,
            ),
            (
                "have-remote-offer->SetLocal(pranswer)->have-local-pranswer",
                SignalingState::HaveRemoteOffer,
                SignalingState::HaveLocalPranswer,
                StateChangeOp::SetLocal,
                SDPType::Pranswer,
                None,
            ),
            (
                "have-local-pranswer->SetLocal(answer)->stable",
                SignalingState::HaveLocalPranswer,
                SignalingState::Stable,
                StateChangeOp::SetLocal,
                SDPType::Answer,
                None,
            ),
            (
                "(invalid) stable->SetRemote(pranswer)->have-remote-pranswer",
                SignalingState::Stable,
                SignalingState::HaveRemotePranswer,
                StateChangeOp::SetRemote,
                SDPType::Pranswer,
                Some(Error::ErrSignalingStateProposedTransitionInvalid),
            ),
            (
                "(invalid) stable->SetRemote(rollback)->have-local-offer",
                SignalingState::Stable,
                SignalingState::HaveLocalOffer,
                StateChangeOp::SetRemote,
                SDPType::Rollback,
                Some(Error::ErrSignalingStateCannotRollback),
            ),
        ];

        for (desc, cur, next, op, sdp_type, expected_err) in tests {
            let result = check_next_signaling_state(cur, next, op, sdp_type);
            match (&result, &expected_err) {
                (Ok(got), None) => {
                    assert_eq!(*got, next, "{} state mismatch", desc);
                }
                (Err(got), Some(err)) => {
                    assert_eq!(err.to_string(), got.to_string(), "{} error mismatch", desc);
                }
                _ => {
                    assert!(
                        false,
                        "{}: expected {:?}, but got {:?}",
                        desc, expected_err, result
                    );
                }
            };
        }
    }
}
