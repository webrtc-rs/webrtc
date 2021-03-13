#[cfg(test)]
mod resolver_test;

use super::errors::*;
use crate::Error;

use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;

pub(crate) struct Resolver {
    parent: Option<Arc<Resolver>>,  // read-only
    hosts: HashMap<String, IpAddr>, // TODO: requires mutex
}

impl Resolver {
    pub(crate) fn new() -> Self {
        let mut r = Resolver {
            parent: None,
            hosts: HashMap::new(),
        };

        if let Err(err) = r.add_host("localhost".to_owned(), "127.0.0.1".to_owned()) {
            log::warn!("failed to add localhost to Resolver: {}", err);
        }
        r
    }

    pub(crate) fn set_parent(&mut self, p: Arc<Resolver>) {
        self.parent = Some(p);
    }

    pub(crate) fn add_host(&mut self, name: String, ip_addr: String) -> Result<(), Error> {
        if name.is_empty() {
            return Err(ERR_HOSTNAME_EMPTY.to_owned());
        }
        let ip = IpAddr::from_str(&ip_addr)?;
        self.hosts.insert(name, ip);

        Ok(())
    }

    pub(crate) fn lookup(&self, host_name: String) -> Option<IpAddr> {
        if let Some(ip) = self.hosts.get(&host_name) {
            return Some(*ip);
        }

        // mutex must be unlocked before calling into parent Resolver
        if let Some(parent) = &self.parent {
            parent.lookup(host_name)
        } else {
            None
        }
    }
}
