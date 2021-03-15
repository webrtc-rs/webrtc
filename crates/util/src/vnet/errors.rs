use crate::Error;

lazy_static! {
    pub static ref ERR_OBS_CANNOT_BE_NIL: Error = Error::new("obs cannot be nil".to_owned());
    pub static ref ERR_USE_CLOSED_NETWORK_CONN: Error =
        Error::new("use of closed network connection".to_owned());
    pub static ref ERR_ADDR_NOT_UDPADDR: Error = Error::new("addr is not a net.UDPAddr".to_owned());
    pub static ref ERR_LOC_ADDR: Error = Error::new("something went wrong with locAddr".to_owned());
    pub static ref ERR_ALREADY_CLOSED: Error = Error::new("already closed".to_owned());
    pub static ref ERR_NO_REM_ADDR: Error = Error::new("no remAddr defined".to_owned());
    pub static ref ERR_ADDRESS_ALREADY_IN_USE: Error =
        Error::new("address already in use".to_owned());
    pub static ref ERR_NO_SUCH_UDPCONN: Error = Error::new("no such UDPConn".to_owned());
    pub static ref ERR_CANNOT_REMOVE_UNSPECIFIED_IP: Error =
        Error::new("cannot remove unspecified IP by the specified IP".to_owned());
    pub static ref ERR_NO_ADDRESS_ASSIGNED: Error = Error::new("no address assigned".to_owned());
    pub static ref ERR_NAT_REQURIES_MAPPING: Error =
        Error::new("1:1 NAT requires more than one mapping".to_owned());
    pub static ref ERR_MISMATCH_LENGTH_IP: Error =
        Error::new("length mismtach between mappedIPs and localIPs".to_owned());
    pub static ref ERR_NON_UDP_TRANSLATION_NOT_SUPPORTED: Error =
        Error::new("non-udp translation is not supported yet".to_owned());
    pub static ref ERR_NO_ASSOCIATED_LOCAL_ADDRESS: Error =
        Error::new("no associated local address".to_owned());
    pub static ref ERR_NO_NAT_BINDING_FOUND: Error = Error::new("no NAT binding found".to_owned());
    pub static ref ERR_HAS_NO_PERMISSION: Error = Error::new("has no permission".to_owned());
    pub static ref ERR_HOSTNAME_EMPTY: Error = Error::new("host name must not be empty".to_owned());
    pub static ref ERR_FAILEDTO_PARSE_IPADDR: Error =
        Error::new("failed to parse IP address".to_owned());
    pub static ref ERR_NO_INTERFACE: Error = Error::new("no interface is available".to_owned());
    pub static ref ERR_NOT_FOUND: Error = Error::new("not found".to_owned());
    pub static ref ERR_UNEXPECTED_NETWORK: Error = Error::new("unexpected network".to_owned());
    pub static ref ERR_CANT_ASSIGN_REQUESTED_ADDR: Error =
        Error::new("can't assign requested address".to_owned());
    pub static ref ERR_UNKNOWN_NETWORK: Error = Error::new("unknown network".to_owned());
    pub static ref ERR_NO_ROUTER_LINKED: Error = Error::new("no router linked".to_owned());
    pub static ref ERR_INVALID_PORT_NUMBER: Error = Error::new("invalid port number".to_owned());
    pub static ref ERR_UNEXPECTED_TYPE_SWITCH_FAILURE: Error =
        Error::new("unexpected type-switch failure".to_owned());
    pub static ref ERR_BIND_FAILER_FOR: Error = Error::new("bind failed for".to_owned());
    pub static ref ERR_END_PORT_LESS_THAN_START: Error =
        Error::new("end port is less than the start".to_owned());
    pub static ref ERR_PORT_SPACE_EXHAUSTED: Error = Error::new("port space exhausted".to_owned());
    pub static ref ERR_VNET_DISABLED: Error = Error::new("vnet is not enabled".to_owned());
    pub static ref ERR_INVALID_LOCAL_IPIN_STATIC_IPS: Error =
        Error::new("invalid local IP in static_ips".to_owned());
    pub static ref ERR_LOCAL_IP_BEYOND_STATIC_IPS_SUBSET: Error =
        Error::new("mapped in static_ips is beyond subnet".to_owned());
    pub static ref ERR_LOCAL_IP_NO_STATICS_IPS_ASSOCIATED: Error =
        Error::new("all static_ips must have associated local IPs".to_owned());
    pub static ref ERR_ROUTER_ALREADY_STARTED: Error =
        Error::new("router already started".to_owned());
    pub static ref ERR_ROUTER_ALREADY_STOPPED: Error =
        Error::new("router already stopped".to_owned());
    pub static ref ERR_STATIC_IP_IS_BEYOND_SUBNET: Error =
        Error::new("static IP is beyond subnet".to_owned());
    pub static ref ERR_ADDRESS_SPACE_EXHAUSTED: Error =
        Error::new("address space exhausted".to_owned());
    pub static ref ERR_NO_IPADDR_ETH0: Error =
        Error::new("no IP address is assigned for eth0".to_owned());
}
