#[cfg(test)]
mod mdns_test;

use mdns::config::*;
use mdns::conn::*;

use uuid::Uuid;

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use util::Error;

// MulticastDNSMode represents the different Multicast modes ICE can run in
// MulticastDNSMode enum
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum MulticastDnsMode {
    Unspecified,

    // MulticastDNSModeDisabled means remote mDNS candidates will be discarded, and local host candidates will use IPs
    Disabled,

    // MulticastDNSModeQueryOnly means remote mDNS candidates will be accepted, and local host candidates will use IPs
    QueryOnly,

    // MulticastDNSModeQueryAndGather means remote mDNS candidates will be accepted, and local host candidates will use mDNS
    QueryAndGather,
}

impl Default for MulticastDnsMode {
    fn default() -> Self {
        MulticastDnsMode::Unspecified
    }
}

pub(crate) fn generate_multicast_dns_name() -> String {
    // https://tools.ietf.org/id/draft-ietf-rtcweb-mdns-ice-candidates-02.html#gathering
    // The unique name MUST consist of a version 4 UUID as defined in [RFC4122], followed by “.local”.
    let u = Uuid::new_v4();
    format!("{}.local", u)
}

pub(crate) fn create_multicast_dns(
    mdns_mode: MulticastDnsMode,
    mdns_name: &str,
) -> Result<Option<Arc<DNSConn>>, Error> {
    if mdns_mode == MulticastDnsMode::Disabled {
        return Ok(None);
    }

    let addr = SocketAddr::from_str(DEFAULT_DEST_ADDR)?;

    match mdns_mode {
        MulticastDnsMode::QueryOnly => {
            let conn = DNSConn::server(addr, Config::default())?;
            Ok(Some(Arc::new(conn)))
        }
        MulticastDnsMode::QueryAndGather => {
            let conn = DNSConn::server(
                addr,
                Config {
                    local_names: vec![mdns_name.to_owned()],
                    ..Default::default()
                },
            )?;
            Ok(Some(Arc::new(conn)))
        }
        _ => Ok(None),
    }
}
