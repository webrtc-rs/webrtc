use crate::errors::*;
use crate::network_type::*;

use std::net::{IpAddr, SocketAddr};
use stun::{agent::*, attributes::*, integrity::*, message::*, textattrs::*, xoraddr::*};

use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::Duration;
use util::{Conn, Error};

pub(crate) fn create_addr(_network: NetworkType, ip: IpAddr, port: u16) -> SocketAddr {
    /*if network.is_tcp(){
        return &net.TCPAddr{IP: ip, Port: port}
    default:
        return &net.UDPAddr{IP: ip, Port: port}
    }*/
    SocketAddr::new(ip, port)
}

pub(crate) fn assert_inbound_username(m: &Message, expected_username: String) -> Result<(), Error> {
    let mut username = Username::new(ATTR_USERNAME, String::new());
    username.get_from(m)?;

    if username.to_string() != expected_username {
        return Err(Error::new(format!(
            "{} expected({}) actual({})",
            ERR_MISMATCH_USERNAME.to_owned(),
            expected_username,
            username,
        )));
    }

    Ok(())
}

pub(crate) fn assert_inbound_message_integrity(m: &mut Message, key: &[u8]) -> Result<(), Error> {
    let message_integrity_attr = MessageIntegrity(key.to_vec());
    message_integrity_attr.check(m)
}

// get_xormapped_addr initiates a stun requests to server_addr using conn, reads the response and returns
// the XORMappedAddress returned by the stun server.
// Adapted from stun v0.2.
pub(crate) async fn get_xormapped_addr(
    conn: &Arc<dyn Conn + Send + Sync>,
    server_addr: SocketAddr,
    deadline: Duration,
) -> Result<XORMappedAddress, Error> {
    let resp = stun_request(conn, server_addr, deadline).await?;
    let mut addr = XORMappedAddress::default();
    addr.get_from(&resp)?;
    Ok(addr)
}

const MAX_MESSAGE_SIZE: usize = 1280;

pub(crate) async fn stun_request(
    conn: &Arc<dyn Conn + Send + Sync>,
    server_addr: SocketAddr,
    deadline: Duration,
) -> Result<Message, Error> {
    let mut req = Message::new();
    req.build(&[Box::new(BINDING_REQUEST), Box::new(TransactionId::new())])?;

    conn.send_to(&req.raw, server_addr).await?;
    let mut bs = vec![0u8; MAX_MESSAGE_SIZE];
    let (n, _) = if deadline > Duration::from_secs(0) {
        match tokio::time::timeout(deadline, conn.recv_from(&mut bs)).await {
            Ok(result) => match result {
                Ok((n, addr)) => (n, addr),
                Err(err) => return Err(Error::new(err.to_string())),
            },
            Err(err) => return Err(Error::new(err.to_string())),
        }
    } else {
        conn.recv_from(&mut bs).await?
    };

    let mut res = Message::new();
    res.raw = bs[..n].to_vec();
    res.decode()?;

    Ok(res)
}

pub(crate) fn local_interfaces(
    interface_filter: &Option<Box<dyn Fn(String) -> bool>>,
    network_types: &[NetworkType],
) -> Result<Vec<IpAddr>, Error> {
    let mut ips = vec![];
    let interfaces = match ifaces::Interface::get_all() {
        Ok(interfaces) => interfaces,
        Err(e) => {
            log::error!("Error getting interfaces: {:?}", e);
            return Err(Error::new(e.to_string()));
        }
    };

    let (mut ipv4requested, mut ipv6requested) = (false, false);
    for typ in network_types {
        if typ.is_ipv4() {
            ipv4requested = true;
        }
        if typ.is_ipv6() {
            ipv6requested = true;
        }
    }

    for iface in interfaces {
        log::debug!("local interface: {:?}", iface);
        /*TODO: if iface.Flags&net.FlagUp == 0 {
            continue // interface down
        }
        if iface.Flags&net.FlagLoopback != 0 {
            continue // loopback interface
        }*/

        if let Some(filter) = interface_filter {
            if !filter(iface.name) {
                continue;
            }
        }

        if let Some(addr) = iface.addr {
            if !addr.ip().is_loopback()
                && ((ipv4requested && addr.is_ipv4()) || (ipv6requested && addr.is_ipv6()))
            {
                ips.push(addr.ip());
            }
        }
    }

    Ok(ips)
}

pub(crate) async fn listen_udp_in_port_range(
    port_max: u16,
    port_min: u16,
    laddr: SocketAddr,
) -> Result<impl Conn, Error> {
    if laddr.port() != 0 || (port_min == 0 && port_max == 0) {
        return Ok(UdpSocket::bind(laddr).await?);
    }
    let i = if port_min == 0 { 1 } else { port_min };
    let j = if port_max == 0 { 0xFFFF } else { port_max };
    if i > j {
        return Err(ERR_PORT.to_owned());
    }

    let port_start = rand::random::<u16>() % (j - i + 1) + i;
    let mut port_current = port_start;
    loop {
        let laddr = SocketAddr::new(laddr.ip(), port_current);
        match UdpSocket::bind(laddr).await {
            Ok(c) => return Ok(c),
            Err(err) => log::debug!("failed to listen {}: {}", laddr, err),
        };

        port_current += 1;
        if port_current > j {
            port_current = i
        }
        if port_current == port_start {
            break;
        }
    }

    return Err(ERR_PORT.to_owned());
}
