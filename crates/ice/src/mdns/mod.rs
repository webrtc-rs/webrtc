#[cfg(test)]
mod mdns_test;

use mdns::config::*;
use mdns::conn::*;

use uuid::Uuid;

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use util::Error;

/// Represents the different Multicast modes that ICE can run.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum MulticastDnsMode {
    Unspecified,

    /// Means remote mDNS candidates will be discarded, and local host candidates will use IPs.
    Disabled,

    /// Means remote mDNS candidates will be accepted, and local host candidates will use IPs.
    QueryOnly,

    /// Means remote mDNS candidates will be accepted, and local host candidates will use mDNS.
    QueryAndGather,
}

impl Default for MulticastDnsMode {
    fn default() -> Self {
        Self::Unspecified
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
) -> Result<Option<Arc<DnsConn>>, Error> {
    if mdns_mode == MulticastDnsMode::Disabled {
        return Ok(None);
    }

    //TODO: make it configurable
    //TODO: why DEFAULT_DEST_ADDR doesn't work on Mac/Win?
    let addr = SocketAddr::from_str("0.0.0.0:5353")?;

    match mdns_mode {
        MulticastDnsMode::QueryOnly => {
            let conn = DnsConn::server(addr, Config::default())?;
            Ok(Some(Arc::new(conn)))
        }
        MulticastDnsMode::QueryAndGather => {
            let conn = DnsConn::server(
                addr,
                Config {
                    local_names: vec![mdns_name.to_owned()],
                    ..Config::default()
                },
            )?;
            Ok(Some(Arc::new(conn)))
        }
        _ => Ok(None),
    }
}
