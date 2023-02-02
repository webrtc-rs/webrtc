use std::fmt;

/// RTPTransceiverDirection indicates the direction of the RTPTransceiver.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCRtpTransceiverDirection {
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
impl From<&str> for RTCRtpTransceiverDirection {
    fn from(raw: &str) -> Self {
        match raw {
            RTP_TRANSCEIVER_DIRECTION_SENDRECV_STR => RTCRtpTransceiverDirection::Sendrecv,
            RTP_TRANSCEIVER_DIRECTION_SENDONLY_STR => RTCRtpTransceiverDirection::Sendonly,
            RTP_TRANSCEIVER_DIRECTION_RECVONLY_STR => RTCRtpTransceiverDirection::Recvonly,
            RTP_TRANSCEIVER_DIRECTION_INACTIVE_STR => RTCRtpTransceiverDirection::Inactive,
            _ => RTCRtpTransceiverDirection::Unspecified,
        }
    }
}

impl From<u8> for RTCRtpTransceiverDirection {
    fn from(v: u8) -> Self {
        match v {
            1 => RTCRtpTransceiverDirection::Sendrecv,
            2 => RTCRtpTransceiverDirection::Sendonly,
            3 => RTCRtpTransceiverDirection::Recvonly,
            4 => RTCRtpTransceiverDirection::Inactive,
            _ => RTCRtpTransceiverDirection::Unspecified,
        }
    }
}

impl fmt::Display for RTCRtpTransceiverDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCRtpTransceiverDirection::Sendrecv => {
                write!(f, "{RTP_TRANSCEIVER_DIRECTION_SENDRECV_STR}")
            }
            RTCRtpTransceiverDirection::Sendonly => {
                write!(f, "{RTP_TRANSCEIVER_DIRECTION_SENDONLY_STR}")
            }
            RTCRtpTransceiverDirection::Recvonly => {
                write!(f, "{RTP_TRANSCEIVER_DIRECTION_RECVONLY_STR}")
            }
            RTCRtpTransceiverDirection::Inactive => {
                write!(f, "{RTP_TRANSCEIVER_DIRECTION_INACTIVE_STR}")
            }
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

impl RTCRtpTransceiverDirection {
    /// reverse indicate the opposite direction
    pub fn reverse(&self) -> RTCRtpTransceiverDirection {
        match *self {
            RTCRtpTransceiverDirection::Sendonly => RTCRtpTransceiverDirection::Recvonly,
            RTCRtpTransceiverDirection::Recvonly => RTCRtpTransceiverDirection::Sendonly,
            _ => *self,
        }
    }

    pub fn intersect(&self, other: RTCRtpTransceiverDirection) -> RTCRtpTransceiverDirection {
        Self::from_send_recv(
            self.has_send() && other.has_send(),
            self.has_recv() && other.has_recv(),
        )
    }

    pub fn from_send_recv(send: bool, recv: bool) -> RTCRtpTransceiverDirection {
        match (send, recv) {
            (true, true) => Self::Sendrecv,
            (true, false) => Self::Sendonly,
            (false, true) => Self::Recvonly,
            (false, false) => Self::Inactive,
        }
    }

    pub fn has_send(&self) -> bool {
        matches!(self, Self::Sendrecv | Self::Sendonly)
    }

    pub fn has_recv(&self) -> bool {
        matches!(self, Self::Sendrecv | Self::Recvonly)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_rtp_transceiver_direction() {
        let tests = vec![
            ("Unspecified", RTCRtpTransceiverDirection::Unspecified),
            ("sendrecv", RTCRtpTransceiverDirection::Sendrecv),
            ("sendonly", RTCRtpTransceiverDirection::Sendonly),
            ("recvonly", RTCRtpTransceiverDirection::Recvonly),
            ("inactive", RTCRtpTransceiverDirection::Inactive),
        ];

        for (ct_str, expected_type) in tests {
            assert_eq!(RTCRtpTransceiverDirection::from(ct_str), expected_type);
        }
    }

    #[test]
    fn test_rtp_transceiver_direction_string() {
        let tests = vec![
            (RTCRtpTransceiverDirection::Unspecified, "Unspecified"),
            (RTCRtpTransceiverDirection::Sendrecv, "sendrecv"),
            (RTCRtpTransceiverDirection::Sendonly, "sendonly"),
            (RTCRtpTransceiverDirection::Recvonly, "recvonly"),
            (RTCRtpTransceiverDirection::Inactive, "inactive"),
        ];

        for (d, expected_string) in tests {
            assert_eq!(d.to_string(), expected_string);
        }
    }

    #[test]
    fn test_rtp_transceiver_has_send() {
        let tests = vec![
            (RTCRtpTransceiverDirection::Unspecified, false),
            (RTCRtpTransceiverDirection::Sendrecv, true),
            (RTCRtpTransceiverDirection::Sendonly, true),
            (RTCRtpTransceiverDirection::Recvonly, false),
            (RTCRtpTransceiverDirection::Inactive, false),
        ];

        for (d, expected_value) in tests {
            assert_eq!(d.has_send(), expected_value);
        }
    }

    #[test]
    fn test_rtp_transceiver_has_recv() {
        let tests = vec![
            (RTCRtpTransceiverDirection::Unspecified, false),
            (RTCRtpTransceiverDirection::Sendrecv, true),
            (RTCRtpTransceiverDirection::Sendonly, false),
            (RTCRtpTransceiverDirection::Recvonly, true),
            (RTCRtpTransceiverDirection::Inactive, false),
        ];

        for (d, expected_value) in tests {
            assert_eq!(d.has_recv(), expected_value);
        }
    }

    #[test]
    fn test_rtp_transceiver_from_send_recv() {
        let tests = vec![
            (RTCRtpTransceiverDirection::Sendrecv, (true, true)),
            (RTCRtpTransceiverDirection::Sendonly, (true, false)),
            (RTCRtpTransceiverDirection::Recvonly, (false, true)),
            (RTCRtpTransceiverDirection::Inactive, (false, false)),
        ];

        for (expected_value, (send, recv)) in tests {
            assert_eq!(
                RTCRtpTransceiverDirection::from_send_recv(send, recv),
                expected_value
            );
        }
    }

    #[test]
    fn test_rtp_transceiver_intersect() {
        use RTCRtpTransceiverDirection::*;

        let tests = vec![
            ((Sendrecv, Recvonly), Recvonly),
            ((Sendrecv, Sendonly), Sendonly),
            ((Sendrecv, Inactive), Inactive),
            ((Sendonly, Inactive), Inactive),
            ((Recvonly, Inactive), Inactive),
            ((Recvonly, Sendrecv), Recvonly),
            ((Sendonly, Sendrecv), Sendonly),
            ((Sendonly, Recvonly), Inactive),
            ((Recvonly, Recvonly), Recvonly),
        ];

        for ((a, b), expected_direction) in tests {
            assert_eq!(a.intersect(b), expected_direction);
        }
    }
}
