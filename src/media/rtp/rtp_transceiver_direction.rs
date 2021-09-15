use std::fmt;

/// RTPTransceiverDirection indicates the direction of the RTPTransceiver.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum RTPTransceiverDirection {
    Unspecified,

    /// Sendrecv indicates the RTPSender will offer
    /// to send RTP and RTPReceiver the will offer to receive RTP.
    Sendrecv,

    /// Sendonly indicates the RTPSender will offer to send RTP.
    Sendonly,

    /// Recvonly indicates the RTPReceiver the will offer to receive RTP.
    Recvonly,

    /// Inactive indicates the RTPSender won't offer
    /// to send RTP and RTPReceiver the won't offer to receive RTP.
    Inactive,
}

const RTP_TRANSCEIVER_DIRECTION_SENDRECV_STR: &str = "sendrecv";
const RTP_TRANSCEIVER_DIRECTION_SENDONLY_STR: &str = "sendonly";
const RTP_TRANSCEIVER_DIRECTION_RECVONLY_STR: &str = "recvonly";
const RTP_TRANSCEIVER_DIRECTION_INACTIVE_STR: &str = "inactive";

/// defines a procedure for creating a new
/// RTPTransceiverDirection from a raw string naming the transceiver direction.
impl From<&str> for RTPTransceiverDirection {
    fn from(raw: &str) -> Self {
        match raw {
            RTP_TRANSCEIVER_DIRECTION_SENDRECV_STR => RTPTransceiverDirection::Sendrecv,
            RTP_TRANSCEIVER_DIRECTION_SENDONLY_STR => RTPTransceiverDirection::Sendonly,
            RTP_TRANSCEIVER_DIRECTION_RECVONLY_STR => RTPTransceiverDirection::Recvonly,
            RTP_TRANSCEIVER_DIRECTION_INACTIVE_STR => RTPTransceiverDirection::Inactive,
            _ => RTPTransceiverDirection::Unspecified,
        }
    }
}

impl From<u8> for RTPTransceiverDirection {
    fn from(v: u8) -> Self {
        match v {
            1 => RTPTransceiverDirection::Sendrecv,
            2 => RTPTransceiverDirection::Sendonly,
            3 => RTPTransceiverDirection::Recvonly,
            4 => RTPTransceiverDirection::Inactive,
            _ => RTPTransceiverDirection::Unspecified,
        }
    }
}

impl fmt::Display for RTPTransceiverDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTPTransceiverDirection::Sendrecv => {
                write!(f, "{}", RTP_TRANSCEIVER_DIRECTION_SENDRECV_STR)
            }
            RTPTransceiverDirection::Sendonly => {
                write!(f, "{}", RTP_TRANSCEIVER_DIRECTION_SENDONLY_STR)
            }
            RTPTransceiverDirection::Recvonly => {
                write!(f, "{}", RTP_TRANSCEIVER_DIRECTION_RECVONLY_STR)
            }
            RTPTransceiverDirection::Inactive => {
                write!(f, "{}", RTP_TRANSCEIVER_DIRECTION_INACTIVE_STR)
            }
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

impl RTPTransceiverDirection {
    /// reverse indicate the opposite direction
    pub fn reverse(&self) -> RTPTransceiverDirection {
        match *self {
            RTPTransceiverDirection::Sendonly => RTPTransceiverDirection::Recvonly,
            RTPTransceiverDirection::Recvonly => RTPTransceiverDirection::Sendonly,
            _ => *self,
        }
    }
}

pub(crate) fn have_rtp_transceiver_direction_intersection(
    haystack: &[RTPTransceiverDirection],
    needle: &[RTPTransceiverDirection],
) -> bool {
    for n in needle {
        for h in haystack {
            if n == h {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_rtp_transceiver_direction() {
        let tests = vec![
            ("Unspecified", RTPTransceiverDirection::Unspecified),
            ("sendrecv", RTPTransceiverDirection::Sendrecv),
            ("sendonly", RTPTransceiverDirection::Sendonly),
            ("recvonly", RTPTransceiverDirection::Recvonly),
            ("inactive", RTPTransceiverDirection::Inactive),
        ];

        for (ct_str, expected_type) in tests {
            assert_eq!(expected_type, RTPTransceiverDirection::from(ct_str));
        }
    }

    #[test]
    fn test_rtp_transceiver_direction_string() {
        let tests = vec![
            (RTPTransceiverDirection::Unspecified, "Unspecified"),
            (RTPTransceiverDirection::Sendrecv, "sendrecv"),
            (RTPTransceiverDirection::Sendonly, "sendonly"),
            (RTPTransceiverDirection::Recvonly, "recvonly"),
            (RTPTransceiverDirection::Inactive, "inactive"),
        ];

        for (d, expected_string) in tests {
            assert_eq!(expected_string, d.to_string());
        }
    }
}
