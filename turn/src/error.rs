use std::num::ParseIntError;
use std::time::SystemTimeError;
use std::{io, net};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    #[error("turn: RelayAddress must be valid IP to use RelayAddressGeneratorStatic")]
    ErrRelayAddressInvalid,
    #[error("turn: PacketConnConfigs and ConnConfigs are empty, unable to proceed")]
    ErrNoAvailableConns,
    #[error("turn: PacketConnConfig must have a non-nil Conn")]
    ErrConnUnset,
    #[error("turn: ListenerConfig must have a non-nil Listener")]
    ErrListenerUnset,
    #[error("turn: RelayAddressGenerator has invalid ListeningAddress")]
    ErrListeningAddressInvalid,
    #[error("turn: RelayAddressGenerator in RelayConfig is unset")]
    ErrRelayAddressGeneratorUnset,
    #[error("turn: max retries exceeded")]
    ErrMaxRetriesExceeded,
    #[error("turn: MaxPort must be not 0")]
    ErrMaxPortNotZero,
    #[error("turn: MaxPort must be not 0")]
    ErrMinPortNotZero,
    #[error("turn: MaxPort less than MinPort")]
    ErrMaxPortLessThanMinPort,
    #[error("turn: relay_conn cannot not be nil")]
    ErrNilConn,
    #[error("turn: TODO")]
    ErrTodo,
    #[error("turn: already listening")]
    ErrAlreadyListening,
    #[error("turn: Server failed to close")]
    ErrFailedToClose,
    #[error("turn: failed to retransmit transaction")]
    ErrFailedToRetransmitTransaction,
    #[error("all retransmissions failed")]
    ErrAllRetransmissionsFailed,
    #[error("no binding found for channel")]
    ErrChannelBindNotFound,
    #[error("STUN server address is not set for the client")]
    ErrStunserverAddressNotSet,
    #[error("only one Allocate() caller is allowed")]
    ErrOneAllocateOnly,
    #[error("already allocated")]
    ErrAlreadyAllocated,
    #[error("non-STUN message from STUN server")]
    ErrNonStunmessage,
    #[error("failed to decode STUN message")]
    ErrFailedToDecodeStun,
    #[error("unexpected STUN request message")]
    ErrUnexpectedStunrequestMessage,
    #[error("channel number not in [0x4000, 0x7FFF]")]
    ErrInvalidChannelNumber,
    #[error("channelData length != len(Data)")]
    ErrBadChannelDataLength,
    #[error("unexpected EOF")]
    ErrUnexpectedEof,
    #[error("invalid value for requested family attribute")]
    ErrInvalidRequestedFamilyValue,
    #[error("error code 443: peer address family mismatch")]
    ErrPeerAddressFamilyMismatch,
    #[error("fake error")]
    ErrFakeErr,
    #[error("try again")]
    ErrTryAgain,
    #[error("use of closed network connection")]
    ErrClosed,
    #[error("addr is not a net.UDPAddr")]
    ErrUdpaddrCast,
    #[error("already closed")]
    ErrAlreadyClosed,
    #[error("try-lock is already locked")]
    ErrDoubleLock,
    #[error("transaction closed")]
    ErrTransactionClosed,
    #[error("wait_for_result called on non-result transaction")]
    ErrWaitForResultOnNonResultTransaction,
    #[error("failed to build refresh request")]
    ErrFailedToBuildRefreshRequest,
    #[error("failed to refresh allocation")]
    ErrFailedToRefreshAllocation,
    #[error("failed to get lifetime from refresh response")]
    ErrFailedToGetLifetime,
    #[error("too short buffer")]
    ErrShortBuffer,
    #[error("unexpected response type")]
    ErrUnexpectedResponse,
    #[error("AllocatePacketConn must be set")]
    ErrAllocatePacketConnMustBeSet,
    #[error("AllocateConn must be set")]
    ErrAllocateConnMustBeSet,
    #[error("LeveledLogger must be set")]
    ErrLeveledLoggerMustBeSet,
    #[error("you cannot use the same channel number with different peer")]
    ErrSameChannelDifferentPeer,
    #[error("allocations must not be created with nil FivTuple")]
    ErrNilFiveTuple,
    #[error("allocations must not be created with nil FiveTuple.src_addr")]
    ErrNilFiveTupleSrcAddr,
    #[error("allocations must not be created with nil FiveTuple.dst_addr")]
    ErrNilFiveTupleDstAddr,
    #[error("allocations must not be created with nil turnSocket")]
    ErrNilTurnSocket,
    #[error("allocations must not be created with a lifetime of 0")]
    ErrLifetimeZero,
    #[error("allocation attempt created with duplicate FiveTuple")]
    ErrDupeFiveTuple,
    #[error("failed to cast net.Addr to *net.UDPAddr")]
    ErrFailedToCastUdpaddr,
    #[error("failed to generate nonce")]
    ErrFailedToGenerateNonce,
    #[error("failed to send error message")]
    ErrFailedToSendError,
    #[error("duplicated Nonce generated, discarding request")]
    ErrDuplicatedNonce,
    #[error("no such user exists")]
    ErrNoSuchUser,
    #[error("unexpected class")]
    ErrUnexpectedClass,
    #[error("unexpected method")]
    ErrUnexpectedMethod,
    #[error("failed to handle")]
    ErrFailedToHandle,
    #[error("unhandled STUN packet")]
    ErrUnhandledStunpacket,
    #[error("unable to handle ChannelData")]
    ErrUnableToHandleChannelData,
    #[error("failed to create stun message from packet")]
    ErrFailedToCreateStunpacket,
    #[error("failed to create channel data from packet")]
    ErrFailedToCreateChannelData,
    #[error("relay already allocated for 5-TUPLE")]
    ErrRelayAlreadyAllocatedForFiveTuple,
    #[error("RequestedTransport must be UDP")]
    ErrRequestedTransportMustBeUdp,
    #[error("no support for DONT-FRAGMENT")]
    ErrNoDontFragmentSupport,
    #[error("Request must not contain RESERVATION-TOKEN and EVEN-PORT")]
    ErrRequestWithReservationTokenAndEvenPort,
    #[error("Request must not contain RESERVATION-TOKEN and REQUESTED-ADDRESS-FAMILY")]
    ErrRequestWithReservationTokenAndReqAddressFamily,
    #[error("no allocation found")]
    ErrNoAllocationFound,
    #[error("unable to handle send-indication, no permission added")]
    ErrNoPermission,
    #[error("packet write smaller than packet")]
    ErrShortWrite,
    #[error("no such channel bind")]
    ErrNoSuchChannelBind,
    #[error("failed writing to socket")]
    ErrFailedWriteSocket,
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
