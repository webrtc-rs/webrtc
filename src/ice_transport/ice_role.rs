use std::fmt;

/// ICERole describes the role ice.Agent is playing in selecting the
/// preferred the candidate pair.
///
/// ## Specifications
///
/// * [MDN]
/// * [W3C]
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Web/API/RTCIceTransport/role
/// [W3C]: https://w3c.github.io/webrtc-pc/#dom-rtcicerole
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCIceRole {
    #[default]
    Unspecified,

    /// ICERoleControlling indicates that the ICE agent that is responsible
    /// for selecting the final choice of candidate pairs and signaling them
    /// through STUN and an updated offer, if needed. In any session, one agent
    /// is always controlling. The other is the controlled agent.
    Controlling,

    /// ICERoleControlled indicates that an ICE agent that waits for the
    /// controlling agent to select the final choice of candidate pairs.
    Controlled,
}

const ICE_ROLE_CONTROLLING_STR: &str = "controlling";
const ICE_ROLE_CONTROLLED_STR: &str = "controlled";

impl From<&str> for RTCIceRole {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_ROLE_CONTROLLING_STR => RTCIceRole::Controlling,
            ICE_ROLE_CONTROLLED_STR => RTCIceRole::Controlled,
            _ => RTCIceRole::Unspecified,
        }
    }
}

impl fmt::Display for RTCIceRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCIceRole::Controlling => write!(f, "{ICE_ROLE_CONTROLLING_STR}"),
            RTCIceRole::Controlled => write!(f, "{ICE_ROLE_CONTROLLED_STR}"),
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_ice_role() {
        let tests = vec![
            ("Unspecified", RTCIceRole::Unspecified),
            ("controlling", RTCIceRole::Controlling),
            ("controlled", RTCIceRole::Controlled),
        ];

        for (role_string, expected_role) in tests {
            assert_eq!(RTCIceRole::from(role_string), expected_role);
        }
    }

    #[test]
    fn test_ice_role_string() {
        let tests = vec![
            (RTCIceRole::Unspecified, "Unspecified"),
            (RTCIceRole::Controlling, "controlling"),
            (RTCIceRole::Controlled, "controlled"),
        ];

        for (proto, expected_string) in tests {
            assert_eq!(proto.to_string(), expected_string);
        }
    }
}
