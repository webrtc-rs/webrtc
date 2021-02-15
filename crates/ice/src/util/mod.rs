use crate::errors::*;
use crate::network_type::*;

use std::net::{IpAddr, SocketAddr};
use stun::{attributes::*, integrity::*, message::*, textattrs::*};

use util::Error;

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
