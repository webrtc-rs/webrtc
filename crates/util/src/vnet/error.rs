use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    // from vnet
    #[error("obs cannot be nil")]
    ErrObsCannotBeNil,
    #[error("se of closed network connection")]
    ErrUseClosedNetworkConn,
    #[error("addr is not a net.UDPAddr")]
    ErrAddrNotUdpAddr,
    #[error("something went wrong with locAddr")]
    ErrLocAddr,
    #[error("already closed")]
    ErrAlreadyClosed,
    #[error("no remAddr defined")]
    ErrNoRemAddr,
    #[error("address already in use")]
    ErrAddressAlreadyInUse,
    #[error("no such UDPConn")]
    ErrNoSuchUdpConn,
    #[error("cannot remove unspecified IP by the specified IP")]
    ErrCannotRemoveUnspecifiedIp,
    #[error("no address assigned")]
    ErrNoAddressAssigned,
    #[error("1:1 NAT requires more than one mapping")]
    ErrNatRequriesMapping,
    #[error("length mismtach between mappedIPs and localIPs")]
    ErrMismatchLengthIp,
    #[error("non-udp translation is not supported yet")]
    ErrNonUdpTranslationNotSupported,
    #[error("no associated local address")]
    ErrNoAssociatedLocalAddress,
    #[error("no NAT binding found")]
    ErrNoNatBindingFound,
    #[error("has no permission")]
    ErrHasNoPermission,
    #[error("host name must not be empty")]
    ErrHostnameEmpty,
    #[error("failed to parse IP address")]
    ErrFailedToParseIpaddr,
    #[error("no interface is available")]
    ErrNoInterface,
    #[error("not found")]
    ErrNotFound,
    #[error("unexpected network")]
    ErrUnexpectedNetwork,
    #[error("can't assign requested address")]
    ErrCantAssignRequestedAddr,
    #[error("unknown network")]
    ErrUnknownNetwork,
    #[error("no router linked")]
    ErrNoRouterLinked,
    #[error("invalid port number")]
    ErrInvalidPortNumber,
    #[error("unexpected type-switch failure")]
    ErrUnexpectedTypeSwitchFailure,
    #[error("bind failed")]
    ErrBindFailed,
    #[error("end port is less than the start")]
    ErrEndPortLessThanStart,
    #[error("port space exhausted")]
    ErrPortSpaceExhausted,
    #[error("vnet is not enabled")]
    ErrVnetDisabled,
    #[error("invalid local IP in static_ips")]
    ErrInvalidLocalIpInStaticIps,
    #[error("mapped in static_ips is beyond subnet")]
    ErrLocalIpBeyondStaticIpsSubset,
    #[error("all static_ips must have associated local IPs")]
    ErrLocalIpNoStaticsIpsAssociated,
    #[error("router already started")]
    ErrRouterAlreadyStarted,
    #[error("router already stopped")]
    ErrRouterAlreadyStopped,
    #[error("static IP is beyond subnet")]
    ErrStaticIpIsBeyondSubnet,
    #[error("address space exhausted")]
    ErrAddressSpaceExhausted,
    #[error("no IP address is assigned for eth0")]
    ErrNoIpaddrEth0,
    #[error("Invalid mask")]
    ErrInvalidMask,

    #[error("Other errors:{0}")]
    ErrOthers(String),
}
