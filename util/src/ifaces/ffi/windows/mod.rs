#![allow(unused, non_upper_case_globals)]

use winapi::shared::basetsd::{UINT32, UINT8, ULONG64};
use winapi::shared::guiddef::GUID;
use winapi::shared::minwindef::{BYTE, DWORD, PULONG, ULONG};
use winapi::shared::ws2def::SOCKET_ADDRESS;
use winapi::um::winnt::{PCHAR, PVOID, PWCHAR, WCHAR};

const MAX_ADAPTER_ADDRESS_LENGTH: usize = 8;
const ZONE_INDICES_LENGTH: usize = 16;
const MAX_DHCPV6_DUID_LENGTH: usize = 130;
const MAX_DNS_SUFFIX_STRING_LENGTH: usize = 256;

pub const IP_ADAPTER_IPV4_ENABLED: DWORD = 0x0080;
pub const IP_ADAPTER_IPV6_ENABLED: DWORD = 0x0100;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::{io, mem, ptr};

use winapi::shared::winerror::{
    ERROR_ADDRESS_NOT_ASSOCIATED, ERROR_BUFFER_OVERFLOW, ERROR_INVALID_PARAMETER,
    ERROR_NOT_ENOUGH_MEMORY, ERROR_NO_DATA, ERROR_SUCCESS,
};
use winapi::shared::ws2def::{AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_IN};
use winapi::shared::ws2ipdef::SOCKADDR_IN6;

const PREALLOC_ADAPTERS_LEN: usize = 15 * 1024;

use crate::ifaces::{Interface, Kind, NextHop};

#[link(name = "iphlpapi")]
extern "system" {
    pub fn GetAdaptersAddresses(
        family: ULONG,
        flags: ULONG,
        reserved: PVOID,
        addresses: *mut u8,
        size: PULONG,
    ) -> ULONG;
}

#[repr(C)]
pub struct IpAdapterAddresses {
    pub head: IpAdapterAddressesHead,
    pub all: IpAdaptersAddressesAll,
    pub xp: IpAdaptersAddressesXp,
    pub vista: IpAdaptersAddressesVista,
}

#[repr(C)]
pub struct IpAdapterAddressesHead {
    pub length: ULONG,
    if_index: DWORD,
}

/// All Windows & Later
#[repr(C)]
pub struct IpAdaptersAddressesAll {
    pub next: *const IpAdapterAddresses,
    pub adapter_name: PCHAR,
    pub first_unicast_address: *const IpAdapterUnicastAddress,
    first_anycast_address: *const IpAdapterAnycastAddress,
    first_multicast_address: *const IpAdapterMulticastAddress,
    first_dns_server_address: *const IpAdapterDnsServerAddress,
    dns_suffix: PWCHAR,
    pub description: PWCHAR,
    friendly_name: PWCHAR,
    pub physical_address: [BYTE; MAX_ADAPTER_ADDRESS_LENGTH],
    pub physical_address_length: DWORD,
    pub flags: DWORD,
    mtu: DWORD,
    pub if_type: DWORD,
    oper_status: IfOperStatus,
}

/// Windows XP & Later
#[repr(C)]
pub struct IpAdaptersAddressesXp {
    pub ipv6_if_index: DWORD,
    pub zone_indices: [DWORD; ZONE_INDICES_LENGTH],
    first_prefix: *const IpAdapterPrefix,
}

/// Windows Vista & Later
#[repr(C)]
pub struct IpAdaptersAddressesVista {
    transmit_link_speed: ULONG64,
    receive_link_speed: ULONG64,
    first_wins_server_address: *const IpAdapterWinsServerAddress,
    first_gateway_address: *const IpAdapterGatewayAddress,
    ipv4_metric: ULONG,
    ipv6_metric: ULONG,
    luid: IfLuid,
    dhcpv4_server: SOCKET_ADDRESS,
    compartment_id: UINT32,
    network_guid: GUID,
    connection_type: NetIfConnectionType,
    tunnel_type: TunnelType,
    dhcpv6_server: SOCKET_ADDRESS,
    dhcpv6_client_duid: [BYTE; MAX_DHCPV6_DUID_LENGTH],
    dhcpv6_client_duid_length: ULONG,
    dhcpv6_iaid: ULONG,
    first_dns_suffix: *const IpAdapterDnsSuffix,
}

#[repr(C)]
pub struct IpAdapterUnicastAddress {
    pub length: ULONG,
    flags: DWORD,
    pub next: *const IpAdapterUnicastAddress,
    pub address: SOCKET_ADDRESS,
    prefix_origin: IpPrefixOrigin,
    suffix_origin: IpSuffixOrigin,
    pub dad_state: IpDadState,
    valid_lifetime: ULONG,
    preferred_lifetime: ULONG,
    lease_lifetime: ULONG,
    on_link_prefix_length: UINT8,
}

#[repr(C)]
pub struct IpAdapterAnycastAddress {
    length: ULONG,
    flags: DWORD,
    next: *const IpAdapterAnycastAddress,
    address: SOCKET_ADDRESS,
}

#[repr(C)]
pub struct IpAdapterMulticastAddress {
    length: ULONG,
    flags: DWORD,
    next: *const IpAdapterMulticastAddress,
    address: SOCKET_ADDRESS,
}

#[repr(C)]
pub struct IpAdapterDnsServerAddress {
    length: ULONG,
    reserved: DWORD,
    next: *const IpAdapterDnsServerAddress,
    address: SOCKET_ADDRESS,
}

#[repr(C)]
pub struct IpAdapterPrefix {
    length: ULONG,
    flags: DWORD,
    next: *const IpAdapterPrefix,
    address: SOCKET_ADDRESS,
    prefix_length: ULONG,
}

#[repr(C)]
pub struct IpAdapterWinsServerAddress {
    length: ULONG,
    reserved: DWORD,
    next: *const IpAdapterWinsServerAddress,
    address: SOCKET_ADDRESS,
}

#[repr(C)]
pub struct IpAdapterGatewayAddress {
    length: ULONG,
    reserved: DWORD,
    next: *const IpAdapterGatewayAddress,
    address: SOCKET_ADDRESS,
}

#[repr(C)]
pub struct IpAdapterDnsSuffix {
    next: *const IpAdapterDnsSuffix,
    string: [WCHAR; MAX_DNS_SUFFIX_STRING_LENGTH],
}

bitflags! {
    struct IfLuid: ULONG64 {
        const Reserved = 0x0000000000FFFFFF;
        const NetLuidIndex = 0x0000FFFFFF000000;
        const IfType = 0xFFFF00000000000;
    }
}

#[repr(C)]
pub enum IpPrefixOrigin {
    IpPrefixOriginOther = 0,
    IpPrefixOriginManual,
    IpPrefixOriginWellKnown,
    IpPrefixOriginDhcp,
    IpPrefixOriginRouterAdvertisement,
    IpPrefixOriginUnchanged = 16,
}

#[repr(C)]
pub enum IpSuffixOrigin {
    IpSuffixOriginOther = 0,
    IpSuffixOriginManual,
    IpSuffixOriginWellKnown,
    IpSuffixOriginDhcp,
    IpSuffixOriginLinkLayerAddress,
    IpSuffixOriginRandom,
    IpSuffixOriginUnchanged = 16,
}

#[derive(PartialEq, Eq)]
#[repr(C)]
pub enum IpDadState {
    IpDadStateInvalid = 0,
    IpDadStateTentative,
    IpDadStateDuplicate,
    IpDadStateDeprecated,
    IpDadStatePreferred,
}

#[repr(C)]
pub enum IfOperStatus {
    IfOperStatusUp = 1,
    IfOperStatusDown = 2,
    IfOperStatusTesting = 3,
    IfOperStatusUnknown = 4,
    IfOperStatusDormant = 5,
    IfOperStatusNotPresent = 6,
    IfOperStatusLowerLayerDown = 7,
}

#[repr(C)]
pub enum NetIfConnectionType {
    NetIfConnectionDedicated = 1,
    NetIfConnectionPassive = 2,
    NetIfConnectionDemand = 3,
    NetIfConnectionMaximum = 4,
}

#[repr(C)]
pub enum TunnelType {
    TunnelTypeNone = 0,
    TunnelTypeOther = 1,
    TunnelTypeDirect = 2,
    TunnelType6To4 = 11,
    TunnelTypeIsatap = 13,
    TunnelTypeTeredo = 14,
    TunnelTypeIpHttps = 15,
}

unsafe fn v4_socket_from_adapter(unicast_addr: &IpAdapterUnicastAddress) -> SocketAddrV4 {
    let socket_addr = &unicast_addr.address;

    let in_addr: SOCKADDR_IN = mem::transmute(*socket_addr.lpSockaddr);
    let sin_addr = in_addr.sin_addr.S_un;

    let v4_addr = Ipv4Addr::new(
        *sin_addr.S_addr() as u8,
        (*sin_addr.S_addr() >> 8) as u8,
        (*sin_addr.S_addr() >> 16) as u8,
        (*sin_addr.S_addr() >> 24) as u8,
    );

    SocketAddrV4::new(v4_addr, 0)
}

unsafe fn v6_socket_from_adapter(unicast_addr: &IpAdapterUnicastAddress) -> SocketAddrV6 {
    let socket_addr = &unicast_addr.address;

    let sock_addr6: *const SOCKADDR_IN6 = socket_addr.lpSockaddr as *const SOCKADDR_IN6;
    let in6_addr: SOCKADDR_IN6 = *sock_addr6;

    let v6_addr = (*in6_addr.sin6_addr.u.Word()).into();

    SocketAddrV6::new(
        v6_addr,
        0,
        in6_addr.sin6_flowinfo,
        *in6_addr.u.sin6_scope_id(),
    )
}

unsafe fn local_ifaces_with_buffer(buffer: &mut Vec<u8>) -> io::Result<()> {
    let mut length = buffer.capacity() as u32;

    let ret_code = GetAdaptersAddresses(
        AF_UNSPEC as u32,
        0,
        ptr::null_mut(),
        buffer.as_mut_ptr(),
        &mut length,
    );
    match ret_code {
        ERROR_SUCCESS => Ok(()),
        ERROR_ADDRESS_NOT_ASSOCIATED => Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "An address has not yet been associated with the network endpoint.",
        )),
        ERROR_BUFFER_OVERFLOW => {
            buffer.reserve_exact(length as usize);

            local_ifaces_with_buffer(buffer)
        }
        ERROR_INVALID_PARAMETER => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "One of the parameters is invalid.",
        )),
        ERROR_NOT_ENOUGH_MEMORY => Err(io::Error::new(
            io::ErrorKind::Other,
            "Insufficient memory resources are available to complete the operation.",
        )),
        ERROR_NO_DATA => Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "No addresses were found for the requested parameters.",
        )),
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            "Some Other Error Occurred.",
        )),
    }
}

unsafe fn map_adapter_addresses(mut adapter_addr: *const IpAdapterAddresses) -> Vec<Interface> {
    let mut adapter_addresses = Vec::new();

    while !adapter_addr.is_null() {
        let curr_adapter_addr = &*adapter_addr;

        let mut unicast_addr = curr_adapter_addr.all.first_unicast_address;
        while !unicast_addr.is_null() {
            let curr_unicast_addr = &*unicast_addr;

            // For some reason, some IpDadState::IpDadStateDeprecated addresses are return
            // These contain BOGUS interface indices and will cause problesm if used
            if curr_unicast_addr.dad_state != IpDadState::IpDadStateDeprecated {
                if is_ipv4_enabled(curr_unicast_addr) {
                    adapter_addresses.push(Interface {
                        name: "".to_string(),
                        kind: Kind::Ipv4,
                        addr: Some(SocketAddr::V4(v4_socket_from_adapter(curr_unicast_addr))),
                        mask: None,
                        hop: None,
                    });
                } else if is_ipv6_enabled(curr_unicast_addr) {
                    let mut v6_sock = v6_socket_from_adapter(curr_unicast_addr);
                    // Make sure the scope id is set for ALL interfaces, not just link-local
                    v6_sock.set_scope_id(curr_adapter_addr.xp.ipv6_if_index);
                    adapter_addresses.push(Interface {
                        name: "".to_string(),
                        kind: Kind::Ipv6,
                        addr: Some(SocketAddr::V6(v6_sock)),
                        mask: None,
                        hop: None,
                    });
                }
            }

            unicast_addr = curr_unicast_addr.next;
        }

        adapter_addr = curr_adapter_addr.all.next;
    }

    adapter_addresses
}

/// Query the local system for all interface addresses.
pub fn ifaces() -> Result<Vec<Interface>, ::std::io::Error> {
    let mut adapters_list = Vec::with_capacity(PREALLOC_ADAPTERS_LEN);
    unsafe {
        local_ifaces_with_buffer(&mut adapters_list)?;

        Ok(map_adapter_addresses(
            adapters_list.as_ptr() as *const IpAdapterAddresses
        ))
    }
}

unsafe fn is_ipv4_enabled(unicast_addr: &IpAdapterUnicastAddress) -> bool {
    if unicast_addr.length != 0 {
        let socket_addr = &unicast_addr.address;
        let sa_family = (*socket_addr.lpSockaddr).sa_family;

        sa_family == AF_INET as u16
    } else {
        false
    }
}

unsafe fn is_ipv6_enabled(unicast_addr: &IpAdapterUnicastAddress) -> bool {
    if unicast_addr.length != 0 {
        let socket_addr = &unicast_addr.address;
        let sa_family = (*socket_addr.lpSockaddr).sa_family;

        sa_family == AF_INET6 as u16
    } else {
        false
    }
}
