use util::Error;

lazy_static! {
    // ErrUnknownType indicates an error with Unknown info.
    pub static ref ERR_UNKNOWN_TYPE:Error = Error::new("Unknown".to_owned());

    // ErrSchemeType indicates the scheme type could not be parsed.
    pub static ref ERR_SCHEME_TYPE:Error = Error::new("unknown scheme type".to_owned());

    // ErrSTUNQuery indicates query arguments are provided in a STUN URL.
    pub static ref ERR_STUN_QUERY:Error = Error::new("queries not supported in stun address".to_owned());

    // ErrInvalidQuery indicates an malformed query is provided.
    pub static ref ERR_INVALID_QUERY:Error = Error::new("invalid query".to_owned());

    // ErrHost indicates malformed hostname is provided.
    pub static ref ERR_HOST:Error = Error::new("invalid hostname".to_owned());

    // ErrPort indicates malformed port is provided.
    pub static ref ERR_PORT:Error = Error::new("invalid port number".to_owned());

    // ErrLocalUfragInsufficientBits indicates local username fragment insufficient bits are provided.
    // Have to be at least 24 bits long
    pub static ref ERR_LOCAL_UFRAG_INSUFFICIENT_BITS:Error = Error::new("local username fragment is less than 24 bits long".to_owned());

    // ErrLocalPwdInsufficientBits indicates local passoword insufficient bits are provided.
    // Have to be at least 128 bits long
    pub static ref ERR_LOCAL_PWD_INSUFFICIENT_BITS:Error = Error::new("local password is less than 128 bits long".to_owned());

    // ErrProtoType indicates an unsupported transport type was provided.
    pub static ref ERR_PROTO_TYPE:Error = Error::new("invalid transport protocol type".to_owned());

    // ErrClosed indicates the agent is closed
    pub static ref ERR_CLOSED:Error = Error::new("the agent is closed".to_owned());

    // ErrNoCandidatePairs indicates agent does not have a valid candidate pair
    pub static ref ERR_NO_CANDIDATE_PAIRS:Error = Error::new("no candidate pairs available".to_owned());

    // ErrCanceledByCaller indicates agent connection was canceled by the caller
    pub static ref ERR_CANCELED_BY_CALLER:Error = Error::new("connecting canceled by caller".to_owned());

    // ErrMultipleStart indicates agent was started twice
    pub static ref ERR_MULTIPLE_START:Error = Error::new("attempted to start agent twice".to_owned());

    // ErrRemoteUfragEmpty indicates agent was started with an empty remote ufrag
    pub static ref ERR_REMOTE_UFRAG_EMPTY:Error = Error::new("remote ufrag is empty".to_owned());

    // ErrRemotePwdEmpty indicates agent was started with an empty remote pwd
    pub static ref ERR_REMOTE_PWD_EMPTY:Error = Error::new("remote pwd is empty".to_owned());

    // ErrNoOnCandidateHandler indicates agent was started without OnCandidate
    pub static ref ERR_NO_ON_CANDIDATE_HANDLER:Error = Error::new("no OnCandidate provided".to_owned());

    // ErrMultipleGatherAttempted indicates GatherCandidates has been called multiple times
    pub static ref ERR_MULTIPLE_GATHER_ATTEMPTED:Error = Error::new("attempting to gather candidates during gathering state".to_owned());

    // ErrUsernameEmpty indicates agent was give TURN URL with an empty Username
    pub static ref ERR_USERNAME_EMPTY:Error = Error::new("username is empty".to_owned());

    // ErrPasswordEmpty indicates agent was give TURN URL with an empty Password
    pub static ref ERR_PASSWORD_EMPTY:Error = Error::new("password is empty".to_owned());

    // ErrAddressParseFailed indicates we were unable to parse a candidate address
    pub static ref ERR_ADDRESS_PARSE_FAILED:Error = Error::new("failed to parse address".to_owned());

    // ErrLiteUsingNonHostCandidates indicates non host candidates were selected for a lite agent
    pub static ref ERR_LITE_USING_NON_HOST_CANDIDATES:Error = Error::new("lite agents must only use host candidates".to_owned());

    // ErrUselessUrlsProvided indicates that one or more URL was provided to the agent but no host
    // candidate required them
    pub static ref ERR_USELESS_URLS_PROVIDED:Error = Error::new("agent does not need URL with selected candidate types".to_owned());

    // ErrUnsupportedNAT1To1IPCandidateType indicates that the specified NAT1To1IPCandidateType is
    // unsupported
    pub static ref ERR_UNSUPPORTED_NAT_1TO1_IP_CANDIDATE_TYPE:Error = Error::new("unsupported 1:1 NAT IP candidate type".to_owned());

    // ErrInvalidNAT1To1IPMapping indicates that the given 1:1 NAT IP mapping is invalid
    pub static ref ERR_INVALID_NAT_1TO1_IP_MAPPING:Error = Error::new("invalid 1:1 NAT IP mapping".to_owned());

    // ErrExternalMappedIPNotFound in NAT1To1IPMapping
    pub static ref ERR_EXTERNAL_MAPPED_IP_NOT_FOUND:Error = Error::new("external mapped IP not found".to_owned());

    // ErrMulticastDNSWithNAT1To1IPMapping indicates that the mDNS gathering cannot be used along
    // with 1:1 NAT IP mapping for host candidate.
    pub static ref ERR_MULTICAST_DNSWITH_NAT_1TO1_IP_MAPPING:Error = Error::new("mDNS gathering cannot be used with 1:1 NAT IP mapping for host candidate".to_owned());

    // ErrIneffectiveNAT1To1IPMappingHost indicates that 1:1 NAT IP mapping for host candidate is
    // requested, but the host candidate type is disabled.
    pub static ref ERR_INEFFECTIVE_NAT_1TO1_IP_MAPPING_HOST:Error = Error::new("1:1 NAT IP mapping for host candidate ineffective".to_owned());

    // ErrIneffectiveNAT1To1IPMappingSrflx indicates that 1:1 NAT IP mapping for srflx candidate is
    // requested, but the srflx candidate type is disabled.
    pub static ref ERR_INEFFECTIVE_NAT_1TO1_IP_MAPPING_SRFLX:Error = Error::new("1:1 NAT IP mapping for srflx candidate ineffective".to_owned());

    // ErrInvalidMulticastDNSHostName indicates an invalid MulticastDNSHostName
    pub static ref ERR_INVALID_MULTICAST_DNSHOST_NAME:Error = Error::new("invalid mDNS HostName, must end with .local and can only contain a single '.'".to_owned());

    // ErrRestartWhenGathering indicates Restart was called when Agent is in GatheringStateGathering
    pub static ref ERR_RESTART_WHEN_GATHERING:Error = Error::new("ICE Agent can not be restarted when gathering".to_owned());

    // ErrRunCanceled indicates a run operation was canceled by its individual done
    pub static ref ERR_RUN_CANCELED:Error = Error::new("run was canceled by done".to_owned());

    // ErrTCPMuxNotInitialized indicates TCPMux is not initialized and that invalidTCPMux is used.
    pub static ref ERR_TCP_MUX_NOT_INITIALIZED:Error = Error::new("TCPMux is not initialized".to_owned());

    // ErrTCPRemoteAddrAlreadyExists indicates we already have the connection with same remote addr.
    pub static ref ERR_TCP_REMOTE_ADDR_ALREADY_EXISTS:Error = Error::new("conn with same remote addr already exists".to_owned());

    pub static ref ERR_SEND_PACKET                   :Error = Error::new("failed to send packet".to_owned());
    pub static ref ERR_ATTRIBUTE_TOO_SHORT_ICE_CANDIDATE:Error = Error::new("attribute not long enough to be ICE candidate".to_owned());
    pub static ref ERR_PARSE_COMPONENT               :Error = Error::new("could not parse component".to_owned());
    pub static ref ERR_PARSE_PRIORITY                :Error = Error::new("could not parse priority".to_owned());
    pub static ref ERR_PARSE_PORT                    :Error = Error::new("could not parse port".to_owned());
    pub static ref ERR_PARSE_RELATED_ADDR             :Error = Error::new("could not parse related addresses".to_owned());
    pub static ref ERR_PARSE_TYPE                 :Error = Error::new("could not parse type".to_owned());
    pub static ref ERR_UNKNOWN_CANDIDATE_TYPE          :Error = Error::new("unknown candidate type".to_owned());
    pub static ref ERR_GET_XOR_MAPPED_ADDR_RESPONSE     :Error = Error::new("failed to get XOR-MAPPED-ADDRESS response".to_owned());
    pub static ref ERR_CONNECTION_ADDR_ALREADY_EXIST   :Error = Error::new("connection with same remote address already exists".to_owned());
    pub static ref ERR_READING_STREAMING_PACKET       :Error = Error::new("error reading streaming packet".to_owned());
    pub static ref ERR_WRITING                      :Error = Error::new("error writing to".to_owned());
    pub static ref ERR_CLOSING_CONNECTION            :Error = Error::new("error closing connection".to_owned());
    pub static ref ERR_DETERMINE_NETWORK_TYPE         :Error = Error::new("unable to determine networkType".to_owned());
    pub static ref ERR_MISSING_PROTOCOL_SCHEME        :Error = Error::new("missing protocol scheme".to_owned());
    pub static ref ERR_TOO_MANY_COLONS_ADDR            :Error = Error::new("too many colons in address".to_owned());
    pub static ref ERR_READ                         :Error = Error::new("unexpected error trying to read".to_owned());
    pub static ref ERR_UNKNOWN_ROLE                  :Error = Error::new("unknown role".to_owned());
    pub static ref ERR_MISMATCH_USERNAME             :Error = Error::new("username mismatch".to_owned());
    pub static ref ERR_ICE_WRITE_STUN_MESSAGE          :Error = Error::new("the ICE conn can't write STUN messages".to_owned());
    pub static ref ERR_INVALID_URL                    :Error = Error::new("invalid url".to_owned());
    pub static ref ERR_URL_PARSE_ERROR                    :Error = Error::new("relative URL without a base".to_owned());
}
