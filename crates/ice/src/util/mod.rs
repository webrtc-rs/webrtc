use crate::network_type::*;

use std::net::{IpAddr, SocketAddr};

pub(crate) fn create_addr(_network: NetworkType, ip: IpAddr, port: u16) -> SocketAddr {
    /*if network.is_tcp(){
        return &net.TCPAddr{IP: ip, Port: port}
    default:
        return &net.UDPAddr{IP: ip, Port: port}
    }*/
    SocketAddr::new(ip, port)
}
