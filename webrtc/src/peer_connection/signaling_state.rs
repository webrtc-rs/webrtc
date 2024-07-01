use std::fmt;

use crate::error::{Error, Result};
use crate::peer_connection::sdp::sdp_type::RTCSdpType;

#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub(crate) enum StateChangeOp {
    #[default]
    SetLocal,
    SetRemote,
}

impl fmt::Display for StateChangeOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            StateChangeOp::SetLocal => write!(f, "SetLocal"),
            StateChangeOp::SetRemote => write!(f, "SetRemote"),
            //_ => write!(f, UNSPECIFIED_STR),
        }
    }
}

/// SignalingState indicates the signaling state of the offer/answer process.
///
/// ## Specifications
///
/// * [MDN]
/// * [W3C]
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Web/API/RTCPeerConnection/signalingState
/// [W3C]: https://w3c.github.io/webrtc-pc/#dom-peerconnection-signaling-state
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCSignalingState {
    #[default]
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

const SIGNALING_STATE_STABLE_STR: &str = "stable";
const SIGNALING_STATE_HAVE_LOCAL_OFFER_STR: &str = "have-local-offer";
const SIGNALING_STATE_HAVE_REMOTE_OFFER_STR: &str = "have-remote-offer";
const SIGNALING_STATE_HAVE_LOCAL_PRANSWER_STR: &str = "have-local-pranswer";
const SIGNALING_STATE_HAVE_REMOTE_PRANSWER_STR: &str = "have-remote-pranswer";
const SIGNALING_STATE_CLOSED_STR: &str = "closed";

impl From<&str> for RTCSignalingState {
    fn from(raw: &str) -> Self {
        match raw {
            SIGNALING_STATE_STABLE_STR => RTCSignalingState::Stable,
            SIGNALING_STATE_HAVE_LOCAL_OFFER_STR => RTCSignalingState::HaveLocalOffer,
            SIGNALING_STATE_HAVE_REMOTE_OFFER_STR => RTCSignalingState::HaveRemoteOffer,
            SIGNALING_STATE_HAVE_LOCAL_PRANSWER_STR => RTCSignalingState::HaveLocalPranswer,
            SIGNALING_STATE_HAVE_REMOTE_PRANSWER_STR => RTCSignalingState::HaveRemotePranswer,
            SIGNALING_STATE_CLOSED_STR => RTCSignalingState::Closed,
            _ => RTCSignalingState::Unspecified,
        }
    }
}

impl fmt::Display for RTCSignalingState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCSignalingState::Stable => write!(f, "{SIGNALING_STATE_STABLE_STR}"),
            RTCSignalingState::HaveLocalOffer => {
                write!(f, "{SIGNALING_STATE_HAVE_LOCAL_OFFER_STR}")
            }
            RTCSignalingState::HaveRemoteOffer => {
                write!(f, "{SIGNALING_STATE_HAVE_REMOTE_OFFER_STR}")
            }
            RTCSignalingState::HaveLocalPranswer => {
                write!(f, "{SIGNALING_STATE_HAVE_LOCAL_PRANSWER_STR}")
            }
            RTCSignalingState::HaveRemotePranswer => {
                write!(f, "{SIGNALING_STATE_HAVE_REMOTE_PRANSWER_STR}")
            }
            RTCSignalingState::Closed => write!(f, "{SIGNALING_STATE_CLOSED_STR}"),
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

impl From<u8> for RTCSignalingState {
    fn from(v: u8) -> Self {
        match v {
            1 => RTCSignalingState::Stable,
            2 => RTCSignalingState::HaveLocalOffer,
            3 => RTCSignalingState::HaveRemoteOffer,
            4 => RTCSignalingState::HaveLocalPranswer,
            5 => RTCSignalingState::HaveRemotePranswer,
            6 => RTCSignalingState::Closed,
            _ => RTCSignalingState::Unspecified,
        }
    }
}

pub(crate) fn check_next_signaling_state(
    cur: RTCSignalingState,
    next: RTCSignalingState,
    op: StateChangeOp,
    sdp_type: RTCSdpType,
) -> Result<RTCSignalingState> {
    // Special case for rollbacks
    if sdp_type == RTCSdpType::Rollback && cur == RTCSignalingState::Stable {
        return Err(Error::ErrSignalingStateCannotRollback);
    }

    // 4.3.1 valid state transitions
    match cur {
        RTCSignalingState::Stable => {
            match op {
                StateChangeOp::SetLocal => {
                    // stable->SetLocal(offer)->have-local-offer
                    if sdp_type == RTCSdpType::Offer && next == RTCSignalingState::HaveLocalOffer {
                        return Ok(next);
                    }
                }
                StateChangeOp::SetRemote => {
                    // stable->SetRemote(offer)->have-remote-offer
                    if sdp_type == RTCSdpType::Offer && next == RTCSignalingState::HaveRemoteOffer {
                        return Ok(next);
                    }
                }
            }
        }
        RTCSignalingState::HaveLocalOffer => {
            if op == StateChangeOp::SetRemote {
                match sdp_type {
                    // have-local-offer->SetRemote(answer)->stable
                    RTCSdpType::Answer => {
                        if next == RTCSignalingState::Stable {
                            return Ok(next);
                        }
                    }
                    // have-local-offer->SetRemote(pranswer)->have-remote-pranswer
                    RTCSdpType::Pranswer => {
                        if next == RTCSignalingState::HaveRemotePranswer {
                            return Ok(next);
                        }
                    }
                    _ => {}
                }
            } else if op == StateChangeOp::SetLocal
                && sdp_type == RTCSdpType::Offer
                && next == RTCSignalingState::HaveLocalOffer
            {
                return Ok(next);
            }
        }
        RTCSignalingState::HaveRemotePranswer => {
            if op == StateChangeOp::SetRemote && sdp_type == RTCSdpType::Answer {
                // have-remote-pranswer->SetRemote(answer)->stable
                if next == RTCSignalingState::Stable {
                    return Ok(next);
                }
            }
        }
        RTCSignalingState::HaveRemoteOffer => {
            if op == StateChangeOp::SetLocal {
                match sdp_type {
                    // have-remote-offer->SetLocal(answer)->stable
                    RTCSdpType::Answer => {
                        if next == RTCSignalingState::Stable {
                            return Ok(next);
                        }
                    }
                    // have-remote-offer->SetLocal(pranswer)->have-local-pranswer
                    RTCSdpType::Pranswer => {
                        if next == RTCSignalingState::HaveLocalPranswer {
                            return Ok(next);
                        }
                    }
                    _ => {}
                }
            }
        }
        RTCSignalingState::HaveLocalPranswer => {
            if op == StateChangeOp::SetLocal && sdp_type == RTCSdpType::Answer {
                // have-local-pranswer->SetLocal(answer)->stable
                if next == RTCSignalingState::Stable {
                    return Ok(next);
                }
            }
        }
        _ => {
            return Err(Error::ErrSignalingStateProposedTransitionInvalid {
                from: cur,
                applying: sdp_type,
                is_local: op == StateChangeOp::SetLocal,
            });
        }
    };

    Err(Error::ErrSignalingStateProposedTransitionInvalid {
        from: cur,
        is_local: op == StateChangeOp::SetLocal,
        applying: sdp_type,
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_signaling_state() {
        let tests = vec![
            ("Unspecified", RTCSignalingState::Unspecified),
            ("stable", RTCSignalingState::Stable),
            ("have-local-offer", RTCSignalingState::HaveLocalOffer),
            ("have-remote-offer", RTCSignalingState::HaveRemoteOffer),
            ("have-local-pranswer", RTCSignalingState::HaveLocalPranswer),
            (
                "have-remote-pranswer",
                RTCSignalingState::HaveRemotePranswer,
            ),
            ("closed", RTCSignalingState::Closed),
        ];

        for (state_string, expected_state) in tests {
            assert_eq!(RTCSignalingState::from(state_string), expected_state);
        }
    }

    #[test]
    fn test_signaling_state_string() {
        let tests = vec![
            (RTCSignalingState::Unspecified, "Unspecified"),
            (RTCSignalingState::Stable, "stable"),
            (RTCSignalingState::HaveLocalOffer, "have-local-offer"),
            (RTCSignalingState::HaveRemoteOffer, "have-remote-offer"),
            (RTCSignalingState::HaveLocalPranswer, "have-local-pranswer"),
            (
                RTCSignalingState::HaveRemotePranswer,
                "have-remote-pranswer",
            ),
            (RTCSignalingState::Closed, "closed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(state.to_string(), expected_string);
        }
    }

    #[test]
    fn test_signaling_state_transitions() {
        let tests = vec![
            (
                "stable->SetLocal(offer)->have-local-offer",
                RTCSignalingState::Stable,
                RTCSignalingState::HaveLocalOffer,
                StateChangeOp::SetLocal,
                RTCSdpType::Offer,
                None,
            ),
            (
                "stable->SetRemote(offer)->have-remote-offer",
                RTCSignalingState::Stable,
                RTCSignalingState::HaveRemoteOffer,
                StateChangeOp::SetRemote,
                RTCSdpType::Offer,
                None,
            ),
            (
                "have-local-offer->SetRemote(answer)->stable",
                RTCSignalingState::HaveLocalOffer,
                RTCSignalingState::Stable,
                StateChangeOp::SetRemote,
                RTCSdpType::Answer,
                None,
            ),
            (
                "have-local-offer->SetRemote(pranswer)->have-remote-pranswer",
                RTCSignalingState::HaveLocalOffer,
                RTCSignalingState::HaveRemotePranswer,
                StateChangeOp::SetRemote,
                RTCSdpType::Pranswer,
                None,
            ),
            (
                "have-remote-pranswer->SetRemote(answer)->stable",
                RTCSignalingState::HaveRemotePranswer,
                RTCSignalingState::Stable,
                StateChangeOp::SetRemote,
                RTCSdpType::Answer,
                None,
            ),
            (
                "have-remote-offer->SetLocal(answer)->stable",
                RTCSignalingState::HaveRemoteOffer,
                RTCSignalingState::Stable,
                StateChangeOp::SetLocal,
                RTCSdpType::Answer,
                None,
            ),
            (
                "have-remote-offer->SetLocal(pranswer)->have-local-pranswer",
                RTCSignalingState::HaveRemoteOffer,
                RTCSignalingState::HaveLocalPranswer,
                StateChangeOp::SetLocal,
                RTCSdpType::Pranswer,
                None,
            ),
            (
                "have-local-pranswer->SetLocal(answer)->stable",
                RTCSignalingState::HaveLocalPranswer,
                RTCSignalingState::Stable,
                StateChangeOp::SetLocal,
                RTCSdpType::Answer,
                None,
            ),
            (
                "(invalid) stable->SetRemote(pranswer)->have-remote-pranswer",
                RTCSignalingState::Stable,
                RTCSignalingState::HaveRemotePranswer,
                StateChangeOp::SetRemote,
                RTCSdpType::Pranswer,
                Some(Error::ErrSignalingStateProposedTransitionInvalid {
                    from: RTCSignalingState::Stable,
                    is_local: false,
                    applying: RTCSdpType::Pranswer,
                }),
            ),
            (
                "(invalid) stable->SetRemote(rollback)->have-local-offer",
                RTCSignalingState::Stable,
                RTCSignalingState::HaveLocalOffer,
                StateChangeOp::SetRemote,
                RTCSdpType::Rollback,
                Some(Error::ErrSignalingStateCannotRollback),
            ),
        ];

        for (desc, cur, next, op, sdp_type, expected_err) in tests {
            let result = check_next_signaling_state(cur, next, op, sdp_type);
            match (&result, &expected_err) {
                (Ok(got), None) => {
                    assert_eq!(*got, next, "{desc} state mismatch");
                }
                (Err(got), Some(err)) => {
                    assert_eq!(got.to_string(), err.to_string(), "{desc} error mismatch");
                }
                _ => {
                    panic!("{desc}: expected {expected_err:?}, but got {result:?}");
                }
            };
        }
    }
}
