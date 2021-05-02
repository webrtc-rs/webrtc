use std::fmt;

/// paramType represents a SCTP INIT/INITACK parameter
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct ParamType(pub(crate) u16);

pub(crate) const HEARTBEAT_INFO: ParamType = ParamType(1);
/// Heartbeat Info [RFC4960]
pub(crate) const IP_V4ADDR: ParamType = ParamType(5);
/// IPv4 IP [RFC4960]
pub(crate) const IP_V6ADDR: ParamType = ParamType(6);
/// IPv6 IP [RFC4960]
pub(crate) const STATE_COOKIE: ParamType = ParamType(7);
/// State Cookie [RFC4960]
pub(crate) const UNRECOGNIZED_PARAM: ParamType = ParamType(8);
/// Unrecognized Parameters [RFC4960]
pub(crate) const COOKIE_PRESERVATIVE: ParamType = ParamType(9);
/// Cookie Preservative [RFC4960]
pub(crate) const HOST_NAME_ADDR: ParamType = ParamType(11);
/// Host Name IP [RFC4960]
pub(crate) const SUPPORTED_ADDR_TYPES: ParamType = ParamType(12);
/// Supported IP Types [RFC4960]
pub(crate) const OUT_SSNRESET_REQ: ParamType = ParamType(13);
/// Outgoing SSN Reset Request Parameter [RFC6525]
pub(crate) const INC_SSNRESET_REQ: ParamType = ParamType(14);
/// Incoming SSN Reset Request Parameter [RFC6525]
pub(crate) const SSN_TSNRESET_REQ: ParamType = ParamType(15);
/// SSN/TSN Reset Request Parameter [RFC6525]
pub(crate) const RECONFIG_RESP: ParamType = ParamType(16);
/// Re-configuration Response Parameter [RFC6525]
pub(crate) const ADD_OUT_STREAMS_REQ: ParamType = ParamType(17);
/// Add Outgoing Streams Request Parameter [RFC6525]
pub(crate) const ADD_INC_STREAMS_REQ: ParamType = ParamType(18);
/// Add Incoming Streams Request Parameter [RFC6525]
pub(crate) const RANDOM: ParamType = ParamType(32770);
/// Random (0x8002) [RFC4805]
pub(crate) const CHUNK_LIST: ParamType = ParamType(32771);
/// Chunk List (0x8003) [RFC4895]
pub(crate) const REQ_HMACALGO: ParamType = ParamType(32772);
/// Requested HMAC Algorithm Parameter (0x8004) [RFC4895]
pub(crate) const PADDING: ParamType = ParamType(32773);
/// Padding (0x8005)
pub(crate) const SUPPORTED_EXT: ParamType = ParamType(32776);
/// Supported Extensions (0x8008) [RFC5061]
pub(crate) const FORWARD_TSNSUPP: ParamType = ParamType(49152);
/// Forward TSN supported (0xC000) [RFC3758]
pub(crate) const ADD_IPADDR: ParamType = ParamType(49153);
/// Add IP IP (0xC001) [RFC5061]
pub(crate) const DEL_IPADDR: ParamType = ParamType(49154);
/// Delete IP IP (0xC002) [RFC5061]
pub(crate) const ERR_CLAUSE_IND: ParamType = ParamType(49155);
/// Error Cause Indication (0xC003) [RFC5061]
pub(crate) const SET_PRI_ADDR: ParamType = ParamType(49156);
/// Set Primary IP (0xC004) [RFC5061]
pub(crate) const SUCCESS_IND: ParamType = ParamType(49157);
/// Success Indication (0xC005) [RFC5061]
pub(crate) const ADAPT_LAYER_IND: ParamType = ParamType(49158);
/// Adaptation Layer Indication (0xC006) [RFC5061]

/*TODO: func parseParamType(raw []byte) (paramType, error) {
    if len(raw) < 2 {
        return paramType(0), errParamPacketTooShort
    }
    return paramType(binary.BigEndian.Uint16(raw)), nil
}*/

impl fmt::Display for ParamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let others = format!("Unknown ParamType: {}", self.0);
        let s = match *self {
            HEARTBEAT_INFO => "Heartbeat Info",
            IP_V4ADDR => "IPv4 IP",
            IP_V6ADDR => "IPv6 IP",
            STATE_COOKIE => "State Cookie",
            UNRECOGNIZED_PARAM => "Unrecognized Parameters",
            COOKIE_PRESERVATIVE => "Cookie Preservative",
            HOST_NAME_ADDR => "Host Name IP",
            SUPPORTED_ADDR_TYPES => "Supported IP Types",
            OUT_SSNRESET_REQ => "Outgoing SSN Reset Request Parameter",
            INC_SSNRESET_REQ => "Incoming SSN Reset Request Parameter",
            SSN_TSNRESET_REQ => "SSN/TSN Reset Request Parameter",
            RECONFIG_RESP => "Re-configuration Response Parameter",
            ADD_OUT_STREAMS_REQ => "Add Outgoing Streams Request Parameter",
            ADD_INC_STREAMS_REQ => "Add Incoming Streams Request Parameter",
            RANDOM => "Random",
            CHUNK_LIST => "Chunk List",
            REQ_HMACALGO => "Requested HMAC Algorithm Parameter",
            PADDING => "Padding",
            SUPPORTED_EXT => "Supported Extensions",
            FORWARD_TSNSUPP => "Forward TSN supported",
            ADD_IPADDR => "Add IP IP",
            DEL_IPADDR => "Delete IP IP",
            ERR_CLAUSE_IND => "Error Cause Indication",
            SET_PRI_ADDR => "Set Primary IP",
            SUCCESS_IND => "Success Indication",
            ADAPT_LAYER_IND => "Adaptation Layer Indication",
            _ => others.as_str(),
        };
        write!(f, "{}", s)
    }
}
