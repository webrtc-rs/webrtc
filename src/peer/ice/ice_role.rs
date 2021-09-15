use std::fmt;

/// ICERole describes the role ice.Agent is playing in selecting the
/// preferred the candidate pair.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ICERole {
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

impl Default for ICERole {
    fn default() -> Self {
        ICERole::Unspecified
    }
}

const ICE_ROLE_CONTROLLING_STR: &str = "controlling";
const ICE_ROLE_CONTROLLED_STR: &str = "controlled";

impl From<&str> for ICERole {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_ROLE_CONTROLLING_STR => ICERole::Controlling,
            ICE_ROLE_CONTROLLED_STR => ICERole::Controlled,
            _ => ICERole::Unspecified,
        }
    }
}

impl fmt::Display for ICERole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ICERole::Controlling => write!(f, "{}", ICE_ROLE_CONTROLLING_STR),
            ICERole::Controlled => write!(f, "{}", ICE_ROLE_CONTROLLED_STR),
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
            ("Unspecified", ICERole::Unspecified),
            ("controlling", ICERole::Controlling),
            ("controlled", ICERole::Controlled),
        ];

        for (role_string, expected_role) in tests {
            assert_eq!(expected_role, ICERole::from(role_string));
        }
    }

    #[test]
    fn test_ice_role_string() {
        let tests = vec![
            (ICERole::Unspecified, "Unspecified"),
            (ICERole::Controlling, "controlling"),
            (ICERole::Controlled, "controlled"),
        ];

        for (proto, expected_string) in tests {
            assert_eq!(expected_string, proto.to_string());
        }
    }
}
