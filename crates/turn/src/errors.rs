use util::Error;

lazy_static! {
    pub static ref ERR_RELAY_ADDRESS_INVALID: Error = Error::new(
        "turn: RelayAddress must be valid IP to use RelayAddressGeneratorStatic".to_owned()
    );
    pub static ref ERR_NO_AVAILABLE_CONNS: Error = Error::new(
        "turn: PacketConnConfigs and ConnConfigs are empty, unable to proceed".to_owned()
    );
    pub static ref ERR_CONN_UNSET: Error =
        Error::new("turn: PacketConnConfig must have a non-nil Conn".to_owned());
    pub static ref ERR_LISTENER_UNSET: Error =
        Error::new("turn: ListenerConfig must have a non-nil Listener".to_owned());
    pub static ref ERR_LISTENING_ADDRESS_INVALID: Error =
        Error::new("turn: RelayAddressGenerator has invalid ListeningAddress".to_owned());
    pub static ref ERR_RELAY_ADDRESS_GENERATOR_UNSET: Error =
        Error::new("turn: RelayAddressGenerator in RelayConfig is unset".to_owned());
    pub static ref ERR_MAX_RETRIES_EXCEEDED: Error =
        Error::new("turn: max retries exceeded".to_owned());
    pub static ref ERR_MAX_PORT_NOT_ZERO: Error =
        Error::new("turn: MaxPort must be not 0".to_owned());
    pub static ref ERR_MIN_PORT_NOT_ZERO: Error =
        Error::new("turn: MaxPort must be not 0".to_owned());
    pub static ref ERR_NIL_CONN: Error = Error::new("turn: conn cannot not be nil".to_owned());
    pub static ref ERR_TODO: Error = Error::new("turn: TODO".to_owned());
    pub static ref ERR_ALREADY_LISTENING: Error = Error::new("turn: already listening".to_owned());
    pub static ref ERR_FAILED_TO_CLOSE: Error =
        Error::new("turn: Server failed to close".to_owned());
    pub static ref ERR_FAILED_TO_RETRANSMIT_TRANSACTION: Error =
        Error::new("turn: failed to retransmit transaction".to_owned());
    pub static ref ERR_ALL_RETRANSMISSIONS_FAILED: Error =
        Error::new("all retransmissions failed for".to_owned());
    pub static ref ERR_CHANNEL_BIND_NOT_FOUND: Error =
        Error::new("no binding found for channel".to_owned());
    pub static ref ERR_STUNSERVER_ADDRESS_NOT_SET: Error =
        Error::new("STUN server address is not set for the client".to_owned());
    pub static ref ERR_ONE_ALLOCATE_ONLY: Error =
        Error::new("only one Allocate() caller is allowed".to_owned());
    pub static ref ERR_ALREADY_ALLOCATED: Error = Error::new("already allocated".to_owned());
    pub static ref ERR_NON_STUNMESSAGE: Error =
        Error::new("non-STUN message from STUN server".to_owned());
    pub static ref ERR_FAILED_TO_DECODE_STUN: Error =
        Error::new("failed to decode STUN message".to_owned());
    pub static ref ERR_UNEXPECTED_STUNREQUEST_MESSAGE: Error =
        Error::new("unexpected STUN request message".to_owned());

    // ErrInvalidChannelNumber means that channel number is not valid as by RFC 5766 Section 11.
    pub static ref ERR_INVALID_CHANNEL_NUMBER: Error =
        Error::new("channel number not in [0x4000, 0x7FFF]".to_owned());
    // ErrBadChannelDataLength means that channel data length is not equal
    // to actual data length.
    pub static ref ERR_BAD_CHANNEL_DATA_LENGTH: Error =
        Error::new("channelData length != len(Data)".to_owned());
    pub static ref ERR_UNEXPECTED_EOF: Error = Error::new("unexpected EOF".to_owned());
    pub static ref ERR_INVALID_REQUESTED_FAMILY_VALUE: Error = Error::new("invalid value for requested family attribute".to_owned());

    pub static ref ERR_FAKE_ERR: Error = Error::new("fake error".to_owned());
    pub static ref ERR_TRY_AGAIN: Error = Error::new("try again".to_owned());
    pub static ref ERR_CLOSED: Error = Error::new("use of closed network connection".to_owned());
    pub static ref ERR_UDPADDR_CAST: Error = Error::new("addr is not a net.UDPAddr".to_owned());
    pub static ref ERR_ALREADY_CLOSED: Error = Error::new("already closed".to_owned());
    pub static ref ERR_DOUBLE_LOCK: Error = Error::new("try-lock is already locked".to_owned());
    pub static ref ERR_TRANSACTION_CLOSED: Error = Error::new("transaction closed".to_owned());
    pub static ref ERR_WAIT_FOR_RESULT_ON_NON_RESULT_TRANSACTION: Error = Error::new("wait_for_result called on non-result transaction".to_owned());
    pub static ref ERR_FAILED_TO_BUILD_REFRESH_REQUEST: Error = Error::new("failed to build refresh request".to_owned());
    pub static ref ERR_FAILED_TO_REFRESH_ALLOCATION: Error = Error::new("failed to refresh allocation".to_owned());
    pub static ref ERR_FAILED_TO_GET_LIFETIME: Error = Error::new("failed to get lifetime from refresh response".to_owned());
    pub static ref ERR_SHORT_BUFFER: Error = Error::new("too short buffer".to_owned());
}
