use std::fmt;

/// paramType represents a SCTP INIT/INITACK parameter
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub(crate) enum ParamType {
    HeartbeatInfo,
    /// Heartbeat Info [RFCRFC4960]
    Ipv4Addr,
    /// IPv4 IP [RFCRFC4960]
    Ipv6Addr,
    /// IPv6 IP [RFCRFC4960]
    StateCookie,
    /// State Cookie [RFCRFC4960]
    UnrecognizedParam,
    /// Unrecognized Parameters [RFCRFC4960]
    CookiePreservative,
    /// Cookie Preservative [RFCRFC4960]
    HostNameAddr,
    /// Host Name IP [RFCRFC4960]
    SupportedAddrTypes,
    /// Supported IP Types [RFCRFC4960]
    OutSsnResetReq,
    /// Outgoing SSN Reset Request Parameter [RFCRFC6525]
    IncSsnResetReq,
    /// Incoming SSN Reset Request Parameter [RFCRFC6525]
    SsnTsnResetReq,
    /// SSN/TSN Reset Request Parameter [RFCRFC6525]
    ReconfigResp,
    /// Re-configuration Response Parameter [RFCRFC6525]
    AddOutStreamsReq,
    /// Add Outgoing Streams Request Parameter [RFCRFC6525]
    AddIncStreamsReq,
    /// Add Incoming Streams Request Parameter [RFCRFC6525]
    Random,
    /// Random (0x8002) [RFCRFC4805]
    ChunkList,
    /// Chunk List (0x8003) [RFCRFC4895]
    ReqHmacAlgo,
    /// Requested HMAC Algorithm Parameter (0x8004) [RFCRFC4895]
    Padding,
    /// Padding (0x8005)
    SupportedExt,
    /// Supported Extensions (0x8008) [RFCRFC5061]
    ForwardTsnSupp,
    /// Forward TSN supported (0xC000) [RFCRFC3758]
    AddIpAddr,
    /// Add IP IP (0xC001) [RFCRFC5061]
    DelIpaddr,
    /// Delete IP IP (0xC002) [RFCRFC5061]
    ErrClauseInd,
    /// Error Cause Indication (0xC003) [RFCRFC5061]
    SetPriAddr,
    /// Set Primary IP (0xC004) [RFCRFC5061]
    SuccessInd,
    /// Success Indication (0xC005) [RFCRFC5061]
    AdaptLayerInd,
    /// Adaptation Layer Indication (0xC006) [RFCRFC5061]
    Unknown {
        param_type: u16,
    },
}

impl fmt::Display for ParamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ParamType::HeartbeatInfo => "Heartbeat Info",
            ParamType::Ipv4Addr => "IPv4 IP",
            ParamType::Ipv6Addr => "IPv6 IP",
            ParamType::StateCookie => "State Cookie",
            ParamType::UnrecognizedParam => "Unrecognized Parameters",
            ParamType::CookiePreservative => "Cookie Preservative",
            ParamType::HostNameAddr => "Host Name IP",
            ParamType::SupportedAddrTypes => "Supported IP Types",
            ParamType::OutSsnResetReq => "Outgoing SSN Reset Request Parameter",
            ParamType::IncSsnResetReq => "Incoming SSN Reset Request Parameter",
            ParamType::SsnTsnResetReq => "SSN/TSN Reset Request Parameter",
            ParamType::ReconfigResp => "Re-configuration Response Parameter",
            ParamType::AddOutStreamsReq => "Add Outgoing Streams Request Parameter",
            ParamType::AddIncStreamsReq => "Add Incoming Streams Request Parameter",
            ParamType::Random => "Random",
            ParamType::ChunkList => "Chunk List",
            ParamType::ReqHmacAlgo => "Requested HMAC Algorithm Parameter",
            ParamType::Padding => "Padding",
            ParamType::SupportedExt => "Supported Extensions",
            ParamType::ForwardTsnSupp => "Forward TSN supported",
            ParamType::AddIpAddr => "Add IP IP",
            ParamType::DelIpaddr => "Delete IP IP",
            ParamType::ErrClauseInd => "Error Cause Indication",
            ParamType::SetPriAddr => "Set Primary IP",
            ParamType::SuccessInd => "Success Indication",
            ParamType::AdaptLayerInd => "Adaptation Layer Indication",
            _ => "Unknown ParamType",
        };
        write!(f, "{s}")
    }
}

impl From<u16> for ParamType {
    fn from(v: u16) -> ParamType {
        match v {
            1 => ParamType::HeartbeatInfo,
            5 => ParamType::Ipv4Addr,
            6 => ParamType::Ipv6Addr,
            7 => ParamType::StateCookie,
            8 => ParamType::UnrecognizedParam,
            9 => ParamType::CookiePreservative,
            11 => ParamType::HostNameAddr,
            12 => ParamType::SupportedAddrTypes,
            13 => ParamType::OutSsnResetReq,
            14 => ParamType::IncSsnResetReq,
            15 => ParamType::SsnTsnResetReq,
            16 => ParamType::ReconfigResp,
            17 => ParamType::AddOutStreamsReq,
            18 => ParamType::AddIncStreamsReq,
            32770 => ParamType::Random,
            32771 => ParamType::ChunkList,
            32772 => ParamType::ReqHmacAlgo,
            32773 => ParamType::Padding,
            32776 => ParamType::SupportedExt,
            49152 => ParamType::ForwardTsnSupp,
            49153 => ParamType::AddIpAddr,
            49154 => ParamType::DelIpaddr,
            49155 => ParamType::ErrClauseInd,
            49156 => ParamType::SetPriAddr,
            49157 => ParamType::SuccessInd,
            49158 => ParamType::AdaptLayerInd,
            unknown => ParamType::Unknown {
                param_type: unknown,
            },
        }
    }
}

impl From<ParamType> for u16 {
    fn from(v: ParamType) -> u16 {
        match v {
            ParamType::HeartbeatInfo => 1,
            ParamType::Ipv4Addr => 5,
            ParamType::Ipv6Addr => 6,
            ParamType::StateCookie => 7,
            ParamType::UnrecognizedParam => 8,
            ParamType::CookiePreservative => 9,
            ParamType::HostNameAddr => 11,
            ParamType::SupportedAddrTypes => 12,
            ParamType::OutSsnResetReq => 13,
            ParamType::IncSsnResetReq => 14,
            ParamType::SsnTsnResetReq => 15,
            ParamType::ReconfigResp => 16,
            ParamType::AddOutStreamsReq => 17,
            ParamType::AddIncStreamsReq => 18,
            ParamType::Random => 32770,
            ParamType::ChunkList => 32771,
            ParamType::ReqHmacAlgo => 32772,
            ParamType::Padding => 32773,
            ParamType::SupportedExt => 32776,
            ParamType::ForwardTsnSupp => 49152,
            ParamType::AddIpAddr => 49153,
            ParamType::DelIpaddr => 49154,
            ParamType::ErrClauseInd => 49155,
            ParamType::SetPriAddr => 49156,
            ParamType::SuccessInd => 49157,
            ParamType::AdaptLayerInd => 49158,
            ParamType::Unknown { param_type, .. } => param_type,
        }
    }
}
