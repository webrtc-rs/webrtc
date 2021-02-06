#[cfg(test)]
mod mdns_test;

use mdns::config::*;
use mdns::conn::*;

use uuid::Uuid;

use std::net::SocketAddr;
use std::str::FromStr;
use util::Error;

// MulticastDNSMode represents the different Multicast modes ICE can run in
// MulticastDNSMode enum
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum MulticastDNSMode {
    // MulticastDNSModeDisabled means remote mDNS candidates will be discarded, and local host candidates will use IPs
    Disabled,

    // MulticastDNSModeQueryOnly means remote mDNS candidates will be accepted, and local host candidates will use IPs
    QueryOnly,

    // MulticastDNSModeQueryAndGather means remote mDNS candidates will be accepted, and local host candidates will use mDNS
    QueryAndGather,
}

impl Default for MulticastDNSMode {
    fn default() -> Self {
        MulticastDNSMode::Disabled
    }
}

pub(crate) fn generate_multicast_dns_name() -> String {
    // https://tools.ietf.org/id/draft-ietf-rtcweb-mdns-ice-candidates-02.html#gathering
    // The unique name MUST consist of a version 4 UUID as defined in [RFC4122], followed by “.local”.
    let u = Uuid::new_v4();
    format!("{}.local", u)
}

pub(crate) fn create_multicast_dns(
    mdns_mode: MulticastDNSMode,
    mdns_name: String,
) -> Result<Option<DNSConn>, Error> {
    if mdns_mode == MulticastDNSMode::Disabled {
        return Ok(None);
    }

    let addr = SocketAddr::from_str(DEFAULT_DEST_ADDR)?;

    match mdns_mode {
        MulticastDNSMode::QueryOnly => {
            let conn = DNSConn::server(addr, Config::default())?;
            Ok(Some(conn))
        }
        MulticastDNSMode::QueryAndGather => {
            let conn = DNSConn::server(
                addr,
                Config {
                    local_names: vec![mdns_name],
                    ..Default::default()
                },
            )?;
            Ok(Some(conn))
        }
        _ => Ok(None),
    }
}
