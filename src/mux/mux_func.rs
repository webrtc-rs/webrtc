/// MatchFunc allows custom logic for mapping packets to an Endpoint
pub type MatchFunc = Box<dyn (Fn(&[u8]) -> bool) + Send + Sync>;

/// match_all always returns true
pub fn match_all(_b: &[u8]) -> bool {
    true
}

/// match_range is a MatchFunc that accepts packets with the first byte in [lower..upper]
pub fn match_range(lower: u8, upper: u8) -> MatchFunc {
    Box::new(move |buf: &[u8]| -> bool {
        if buf.is_empty() {
            return false;
        }
        let b = buf[0];
        b >= lower && b <= upper
    })
}

/// MatchFuncs as described in RFC7983
/// <https://tools.ietf.org/html/rfc7983>
///              +----------------+
///              |        [0..3] -+--> forward to STUN
///              |                |
///              |      [16..19] -+--> forward to ZRTP
///              |                |
///  packet -->  |      [20..63] -+--> forward to DTLS
///              |                |
///              |      [64..79] -+--> forward to TURN Channel
///              |                |
///              |    [128..191] -+--> forward to RTP/RTCP
///              +----------------+
/// match_dtls is a MatchFunc that accepts packets with the first byte in [20..63]
/// as defined in RFC7983
pub fn match_dtls(b: &[u8]) -> bool {
    match_range(20, 63)(b)
}

// match_srtp_or_srtcp is a MatchFunc that accepts packets with the first byte in [128..191]
// as defined in RFC7983
pub fn match_srtp_or_srtcp(b: &[u8]) -> bool {
    match_range(128, 191)(b)
}

pub(crate) fn is_rtcp(buf: &[u8]) -> bool {
    // Not long enough to determine RTP/RTCP
    if buf.len() < 4 {
        return false;
    }

    let rtcp_packet_type = buf[1];
    (192..=223).contains(&rtcp_packet_type)
}

/// match_srtp is a MatchFunc that only matches SRTP and not SRTCP
pub fn match_srtp(buf: &[u8]) -> bool {
    match_srtp_or_srtcp(buf) && !is_rtcp(buf)
}

/// match_srtcp is a MatchFunc that only matches SRTCP and not SRTP
pub fn match_srtcp(buf: &[u8]) -> bool {
    match_srtp_or_srtcp(buf) && is_rtcp(buf)
}
