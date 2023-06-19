use std::num::ParseIntError;
use std::time::SystemTimeError;
use std::{io, net};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// Indicates an error with Unknown info.
    #[error("Unknown type")]
    ErrUnknownType,

    /// Indicates the scheme type could not be parsed.
    #[error("unknown scheme type")]
    ErrSchemeType,

    /// Indicates query arguments are provided in a STUN URL.
    #[error("queries not supported in stun address")]
    ErrStunQuery,

    /// Indicates an malformed query is provided.
    #[error("invalid query")]
    ErrInvalidQuery,

    /// Indicates malformed hostname is provided.
    #[error("invalid hostname")]
    ErrHost,

    /// Indicates malformed port is provided.
    #[error("invalid port number")]
    ErrPort,

    /// Indicates local username fragment insufficient bits are provided.
    /// Have to be at least 24 bits long.
    #[error("local username fragment is less than 24 bits long")]
    ErrLocalUfragInsufficientBits,

    /// Indicates local passoword insufficient bits are provided.
    /// Have to be at least 128 bits long.
    #[error("local password is less than 128 bits long")]
    ErrLocalPwdInsufficientBits,

    /// Indicates an unsupported transport type was provided.
    #[error("invalid transport protocol type")]
    ErrProtoType,

    /// Indicates the agent is closed.
    #[error("the agent is closed")]
    ErrClosed,

    /// Indicates agent does not have a valid candidate pair.
    #[error("no candidate pairs available")]
    ErrNoCandidatePairs,

    /// Indicates agent connection was canceled by the caller.
    #[error("connecting canceled by caller")]
    ErrCanceledByCaller,

    /// Indicates agent was started twice.
    #[error("attempted to start agent twice")]
    ErrMultipleStart,

    /// Indicates agent was started with an empty remote ufrag.
    #[error("remote ufrag is empty")]
    ErrRemoteUfragEmpty,

    /// Indicates agent was started with an empty remote pwd.
    #[error("remote pwd is empty")]
    ErrRemotePwdEmpty,

    /// Indicates agent was started without on_candidate.
    #[error("no on_candidate provided")]
    ErrNoOnCandidateHandler,

    /// Indicates GatherCandidates has been called multiple times.
    #[error("attempting to gather candidates during gathering state")]
    ErrMultipleGatherAttempted,

    /// Indicates agent was give TURN URL with an empty Username.
    #[error("username is empty")]
    ErrUsernameEmpty,

    /// Indicates agent was give TURN URL with an empty Password.
    #[error("password is empty")]
    ErrPasswordEmpty,

    /// Indicates we were unable to parse a candidate address.
    #[error("failed to parse address")]
    ErrAddressParseFailed,

    /// Indicates that non host candidates were selected for a lite agent.
    #[error("lite agents must only use host candidates")]
    ErrLiteUsingNonHostCandidates,

    /// Indicates that one or more URL was provided to the agent but no host candidate required them.
    #[error("agent does not need URL with selected candidate types")]
    ErrUselessUrlsProvided,

    /// Indicates that the specified NAT1To1IPCandidateType is unsupported.
    #[error("unsupported 1:1 NAT IP candidate type")]
    ErrUnsupportedNat1to1IpCandidateType,

    /// Indicates that the given 1:1 NAT IP mapping is invalid.
    #[error("invalid 1:1 NAT IP mapping")]
    ErrInvalidNat1to1IpMapping,

    /// IPNotFound in NAT1To1IPMapping.
    #[error("external mapped IP not found")]
    ErrExternalMappedIpNotFound,

    /// Indicates that the mDNS gathering cannot be used along with 1:1 NAT IP mapping for host
    /// candidate.
    #[error("mDNS gathering cannot be used with 1:1 NAT IP mapping for host candidate")]
    ErrMulticastDnsWithNat1to1IpMapping,

    /// Indicates that 1:1 NAT IP mapping for host candidate is requested, but the host candidate
    /// type is disabled.
    #[error("1:1 NAT IP mapping for host candidate ineffective")]
    ErrIneffectiveNat1to1IpMappingHost,

    /// Indicates that 1:1 NAT IP mapping for srflx candidate is requested, but the srflx candidate
    /// type is disabled.
    #[error("1:1 NAT IP mapping for srflx candidate ineffective")]
    ErrIneffectiveNat1to1IpMappingSrflx,

    /// Indicates an invalid MulticastDNSHostName.
    #[error("invalid mDNS HostName, must end with .local and can only contain a single '.'")]
    ErrInvalidMulticastDnshostName,

    /// Indicates Restart was called when Agent is in GatheringStateGathering.
    #[error("ICE Agent can not be restarted when gathering")]
    ErrRestartWhenGathering,

    /// Indicates a run operation was canceled by its individual done.
    #[error("run was canceled by done")]
    ErrRunCanceled,

    /// Initialized Indicates TCPMux is not initialized and that invalidTCPMux is used.
    #[error("TCPMux is not initialized")]
    ErrTcpMuxNotInitialized,

    /// Indicates we already have the connection with same remote addr.
    #[error("conn with same remote addr already exists")]
    ErrTcpRemoteAddrAlreadyExists,

    #[error("failed to send packet")]
    ErrSendPacket,
    #[error("attribute not long enough to be ICE candidate")]
    ErrAttributeTooShortIceCandidate,
    #[error("could not parse component")]
    ErrParseComponent,
    #[error("could not parse priority")]
    ErrParsePriority,
    #[error("could not parse port")]
    ErrParsePort,
    #[error("could not parse related addresses")]
    ErrParseRelatedAddr,
    #[error("could not parse type")]
    ErrParseType,
    #[error("unknown candidate type")]
    ErrUnknownCandidateType,
    #[error("failed to get XOR-MAPPED-ADDRESS response")]
    ErrGetXorMappedAddrResponse,
    #[error("connection with same remote address already exists")]
    ErrConnectionAddrAlreadyExist,
    #[error("error reading streaming packet")]
    ErrReadingStreamingPacket,
    #[error("error writing to")]
    ErrWriting,
    #[error("error closing connection")]
    ErrClosingConnection,
    #[error("unable to determine networkType")]
    ErrDetermineNetworkType,
    #[error("missing protocol scheme")]
    ErrMissingProtocolScheme,
    #[error("too many colons in address")]
    ErrTooManyColonsAddr,
    #[error("unexpected error trying to read")]
    ErrRead,
    #[error("unknown role")]
    ErrUnknownRole,
    #[error("username mismatch")]
    ErrMismatchUsername,
    #[error("the ICE conn can't write STUN messages")]
    ErrIceWriteStunMessage,
    #[error("invalid url")]
    ErrInvalidUrl,
    #[error("relative URL without a base")]
    ErrUrlParse,
    #[error("Candidate IP could not be found")]
    ErrCandidateIpNotFound,

    #[error("parse int: {0}")]
    ParseInt(#[from] ParseIntError),
    #[error("parse addr: {0}")]
    ParseIp(#[from] net::AddrParseError),
    #[error("{0}")]
    Io(#[source] IoError),
    #[error("{0}")]
    Util(#[from] util::Error),
    #[error("{0}")]
    Stun(#[from] stun::Error),
    #[error("{0}")]
    ParseUrl(#[from] url::ParseError),
    #[error("{0}")]
    Mdns(#[from] mdns::Error),
    #[error("{0}")]
    Turn(#[from] turn::Error),

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Error)]
#[error("io error: {0}")]
pub struct IoError(#[from] pub io::Error);

// Workaround for wanting PartialEq for io::Error.
impl PartialEq for IoError {
    fn eq(&self, other: &Self) -> bool {
        self.0.kind() == other.0.kind()
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(IoError(e))
    }
}

impl From<SystemTimeError> for Error {
    fn from(e: SystemTimeError) -> Self {
        Error::Other(e.to_string())
    }
}
