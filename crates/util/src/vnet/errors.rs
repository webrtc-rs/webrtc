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
}
