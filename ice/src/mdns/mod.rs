#[cfg(test)]
mod mdns_test;

use crate::error::Result;

use mdns::config::*;
use mdns::conn::*;

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

/// Represents the different Multicast modes that ICE can run.
#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum MulticastDnsMode {
    /// Means remote mDNS candidates will be discarded, and local host candidates will use IPs.
    Disabled,

    /// Means remote mDNS candidates will be accepted, and local host candidates will use IPs.
    QueryOnly,

    /// Means remote mDNS candidates will be accepted, and local host candidates will use mDNS.
    QueryAndGather,
}

impl Default for MulticastDnsMode {
    fn default() -> Self {
        Self::QueryOnly
    }
}

pub(crate) fn generate_multicast_dns_name() -> String {
    // https://tools.ietf.org/id/draft-ietf-rtcweb-mdns-ice-candidates-02.html#gathering
    // The unique name MUST consist of a version 4 UUID as defined in [RFC4122], followed by “.local”.
    let u = Uuid::new_v4();
    format!("{u}.local")
}

pub(crate) fn create_multicast_dns(
    mdns_mode: MulticastDnsMode,
    mdns_name: &str,
    dest_addr: &str,
) -> Result<Option<Arc<DnsConn>>> {
    let local_names = match mdns_mode {
        MulticastDnsMode::QueryOnly => vec![],
        MulticastDnsMode::QueryAndGather => vec![mdns_name.to_owned()],
        MulticastDnsMode::Disabled => return Ok(None),
    };

    let addr = if dest_addr.is_empty() {
        //TODO: why DEFAULT_DEST_ADDR doesn't work on Mac/Win?
        if cfg!(target_os = "linux") {
            SocketAddr::from_str(DEFAULT_DEST_ADDR)?
        } else {
            SocketAddr::from_str("0.0.0.0:5353")?
        }
    } else {
        SocketAddr::from_str(dest_addr)?
    };
    log::info!("mDNS is using {} as dest_addr", addr);

    let conn = DnsConn::server(
        addr,
        Config {
            local_names,
            ..Config::default()
        },
    )?;
    Ok(Some(Arc::new(conn)))
}
