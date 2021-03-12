#[cfg(test)]
mod nat_test;

use super::errors::*;
use crate::Error;

use crate::vnet::chunk::Chunk;
use crate::vnet::net::UDP_STR;
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::ops::Add;
use std::time::SystemTime;
use tokio::time::Duration;

const DEFAULT_NAT_MAPPING_LIFE_TIME: Duration = Duration::from_secs(30);

// EndpointDependencyType defines a type of behavioral dependendency on the
// remote endpoint's IP address or port number. This is used for the two
// kinds of behaviors:
//  - Port Mapping behavior
//  - Filtering behavior
// See: https://tools.ietf.org/html/rfc4787
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum EndpointDependencyType {
    // EndpointIndependent means the behavior is independent of the endpoint's address or port
    EndpointIndependent,
    // EndpointAddrDependent means the behavior is dependent on the endpoint's address
    EndpointAddrDependent,
    // EndpointAddrPortDependent means the behavior is dependent on the endpoint's address and port
    EndpointAddrPortDependent,
}

impl Default for EndpointDependencyType {
    fn default() -> Self {
        EndpointDependencyType::EndpointIndependent
    }
}

// NATMode defines basic behavior of the NAT
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum NATMode {
    // NATModeNormal means the NAT behaves as a standard NAPT (RFC 2663).
    Normal,
    // NATModeNAT1To1 exhibits 1:1 DNAT where the external IP address is statically mapped to
    // a specific local IP address with port number is preserved always between them.
    // When this mode is selected, mapping_behavior, filtering_behavior, port_preservation and
    // mapping_life_time of NATType are ignored.
    NAT1To1,
}

impl Default for NATMode {
    fn default() -> Self {
        NATMode::Normal
    }
}

// NATType has a set of parameters that define the behavior of NAT.
#[derive(Default, Debug, Copy, Clone)]
pub struct NATType {
    mode: NATMode,
    mapping_behavior: EndpointDependencyType,
    filtering_behavior: EndpointDependencyType,
    hair_pining: bool,       // Not implemented yet
    port_preservation: bool, // Not implemented yet
    mapping_life_time: Duration,
}

#[derive(Default, Debug, Clone)]
struct NatConfig {
    name: String,
    nat_type: NATType,
    mapped_ips: Vec<IpAddr>, // mapped IPv4
    local_ips: Vec<IpAddr>,  // local IPv4, required only when the mode is NATModeNAT1To1
}

#[derive(Debug, Clone)]
struct Mapping {
    proto: String,            // "udp" or "tcp"
    local: String,            // "<local-ip>:<local-port>"
    mapped: String,           // "<mapped-ip>:<mapped-port>"
    bound: String,            // key: "[<remote-ip>[:<remote-port>]]"
    filters: HashSet<String>, // key: "[<remote-ip>[:<remote-port>]]"
    expires: SystemTime,      // time to expire
}

impl Default for Mapping {
    fn default() -> Self {
        Mapping {
            proto: String::new(),       // "udp" or "tcp"
            local: String::new(),       // "<local-ip>:<local-port>"
            mapped: String::new(),      // "<mapped-ip>:<mapped-port>"
            bound: String::new(),       // key: "[<remote-ip>[:<remote-port>]]"
            filters: HashSet::new(),    // key: "[<remote-ip>[:<remote-port>]]"
            expires: SystemTime::now(), // time to expire
        }
    }
}

#[derive(Default, Debug, Clone)]
struct NetworkAddressTranslator {
    name: String,
    nat_type: NATType,
    mapped_ips: Vec<IpAddr>,                // mapped IPv4
    local_ips: Vec<IpAddr>, // local IPv4, required only when the mode is NATModeNAT1To1
    outbound_map: HashMap<String, Mapping>, // key: "<proto>:<local-ip>:<local-port>[:remote-ip[:remote-port]]
    inbound_map: HashMap<String, Mapping>,  // key: "<proto>:<mapped-ip>:<mapped-port>"
    udp_port_counter: u16,
    //mutex          sync.RWMutex
}

impl NetworkAddressTranslator {
    pub(crate) fn new(config: NatConfig) -> Result<Self, Error> {
        let mut nat_type = config.nat_type;

        if nat_type.mode == NATMode::NAT1To1 {
            // 1:1 NAT behavior
            nat_type.mapping_behavior = EndpointDependencyType::EndpointIndependent;
            nat_type.filtering_behavior = EndpointDependencyType::EndpointIndependent;
            nat_type.port_preservation = true;
            nat_type.mapping_life_time = Duration::from_secs(0);

            if config.mapped_ips.is_empty() {
                return Err(ERR_NAT_REQURIES_MAPPING.to_owned());
            }
            if config.mapped_ips.len() != config.local_ips.len() {
                return Err(ERR_MISMATCH_LENGTH_IP.to_owned());
            }
        } else {
            // Normal (NAPT) behavior
            nat_type.mode = NATMode::Normal;
            if nat_type.mapping_life_time == Duration::from_secs(0) {
                nat_type.mapping_life_time = DEFAULT_NAT_MAPPING_LIFE_TIME;
            }
        }

        Ok(NetworkAddressTranslator {
            name: config.name,
            nat_type,
            mapped_ips: config.mapped_ips,
            local_ips: config.local_ips,
            outbound_map: HashMap::new(),
            inbound_map: HashMap::new(),
            udp_port_counter: 0,
        })
    }

    pub(crate) fn get_paired_mapped_ip(&self, loc_ip: &IpAddr) -> Option<&IpAddr> {
        for (i, ip) in self.local_ips.iter().enumerate() {
            if ip == loc_ip {
                return self.mapped_ips.get(i);
            }
        }
        None
    }

    pub(crate) fn get_paired_local_ip(&self, mapped_ip: &IpAddr) -> Option<&IpAddr> {
        for (i, ip) in self.mapped_ips.iter().enumerate() {
            if ip == mapped_ip {
                return self.local_ips.get(i);
            }
        }
        None
    }

    pub(crate) fn translate_outbound(
        &mut self,
        from: &dyn Chunk,
    ) -> Result<Option<Box<dyn Chunk>>, Error> {
        //TODO: n.mutex.Lock()
        //defer n.mutex.Unlock()

        let mut to = from.clone_to();

        if from.network() == UDP_STR {
            if self.nat_type.mode == NATMode::NAT1To1 {
                // 1:1 NAT behavior
                let src_addr = from.source_addr();
                if let Some(src_ip) = self.get_paired_mapped_ip(&src_addr.ip()) {
                    to.set_source_addr(&format!("{}:{}", src_ip, src_addr.port()))?;
                } else {
                    log::debug!(
                        "[{}] drop outbound chunk {} with not route",
                        self.name,
                        from
                    );
                    return Ok(None); // silently discard
                }
            } else {
                // Normal (NAPT) behavior
                let bound = match self.nat_type.mapping_behavior {
                    EndpointDependencyType::EndpointIndependent => "".to_owned(),
                    EndpointDependencyType::EndpointAddrDependent => {
                        from.get_destination_ip().to_string()
                    }
                    EndpointDependencyType::EndpointAddrPortDependent => {
                        from.destination_addr().to_string()
                    }
                };

                let filter_key = match self.nat_type.filtering_behavior {
                    EndpointDependencyType::EndpointIndependent => "".to_owned(),
                    EndpointDependencyType::EndpointAddrDependent => {
                        from.get_destination_ip().to_string()
                    }
                    EndpointDependencyType::EndpointAddrPortDependent => {
                        from.destination_addr().to_string()
                    }
                };

                let o_key = format!("udp:{}:{}", from.source_addr(), bound);
                let name = self.name.clone();

                let m_mapped = if let Some(m) = self.find_outbound_mapping(&o_key) {
                    if !m.filters.contains(&filter_key) {
                        log::debug!(
                            "[{}] permit access from {} to {}",
                            name,
                            filter_key,
                            m.mapped
                        );
                        m.filters.insert(filter_key);
                    }
                    m.mapped.clone()
                } else {
                    // Create a new Mapping
                    let mapped_port = 0xC000 + self.udp_port_counter;
                    if self.udp_port_counter == 0xFFFF - 0xC000 {
                        self.udp_port_counter = 0;
                    } else {
                        self.udp_port_counter += 1;
                    }

                    let mut m = if let Some(mapped_ips_first) = self.mapped_ips.first() {
                        Mapping {
                            proto: "udp".to_owned(),
                            local: from.source_addr().to_string(),
                            bound,
                            mapped: format!("{}:{}", mapped_ips_first, mapped_port),
                            filters: HashSet::new(),
                            expires: SystemTime::now().add(self.nat_type.mapping_life_time),
                        }
                    } else {
                        return Err(ERR_NAT_REQURIES_MAPPING.to_owned());
                    };

                    self.outbound_map.insert(o_key.clone(), m.clone());

                    let i_key = format!("udp:{}", m.mapped);

                    log::debug!(
                        "[{}] created a new NAT binding oKey={} i_key={}",
                        self.name,
                        o_key,
                        i_key
                    );
                    log::debug!(
                        "[{}] permit access from {} to {}",
                        self.name,
                        filter_key,
                        m.mapped
                    );

                    m.filters.insert(filter_key);

                    let m_mapped = m.mapped.clone();
                    self.inbound_map.insert(i_key, m);
                    m_mapped
                };

                to.set_source_addr(&m_mapped)?;
            }

            log::debug!(
                "[{}] translate outbound chunk from {} to {}",
                self.name,
                from,
                to
            );

            return Ok(Some(to));
        }

        Err(ERR_NON_UDP_TRANSLATION_NOT_SUPPORTED.to_owned())
    }

    pub(crate) fn translate_inbound(
        &mut self,
        from: &dyn Chunk,
    ) -> Result<Option<Box<dyn Chunk>>, Error> {
        //TODO: n.mutex.Lock()
        //defer n.mutex.Unlock()

        let mut to = from.clone_to();

        if from.network() == UDP_STR {
            if self.nat_type.mode == NATMode::NAT1To1 {
                // 1:1 NAT behavior
                let dst_addr = from.destination_addr();
                if let Some(dst_ip) = self.get_paired_local_ip(&dst_addr.ip()) {
                    let dst_port = from.destination_addr().port();
                    to.set_destination_addr(&format!("{}:{}", dst_ip, dst_port))?;
                } else {
                    return Err(Error::new(format!(
                        "drop {} as {}",
                        from, *ERR_NO_ASSOCIATED_LOCAL_ADDRESS
                    )));
                }
            } else {
                // Normal (NAPT) behavior
                let filter_key = match self.nat_type.filtering_behavior {
                    EndpointDependencyType::EndpointIndependent => "".to_owned(),
                    EndpointDependencyType::EndpointAddrDependent => {
                        from.get_source_ip().to_string()
                    }
                    EndpointDependencyType::EndpointAddrPortDependent => {
                        from.source_addr().to_string()
                    }
                };

                let i_key = format!("udp:{}", from.destination_addr());
                if let Some(m) = self.find_inbound_mapping(&i_key) {
                    if !m.filters.contains(&filter_key) {
                        return Err(Error::new(format!(
                            "drop {} as the remote {} {}",
                            from, filter_key, *ERR_HAS_NO_PERMISSION
                        )));
                    }

                    // See RFC 4847 Section 4.3.  Mapping Refresh
                    // a) Inbound refresh may be useful for applications with no outgoing
                    //   UDP traffic.  However, allowing inbound refresh may allow an
                    //   external attacker or misbehaving application to keep a Mapping
                    //   alive indefinitely.  This may be a security risk.  Also, if the
                    //   process is repeated with different ports, over time, it could
                    //   use up all the ports on the NAT.

                    to.set_destination_addr(&m.local)?;
                } else {
                    return Err(Error::new(format!(
                        "drop {} as {}",
                        from, *ERR_NO_NAT_BINDING_FOUND
                    )));
                }
            }

            log::debug!(
                "[{}] translate inbound chunk from {} to {}",
                self.name,
                from,
                to
            );

            return Ok(Some(to));
        }

        Err(ERR_NON_UDP_TRANSLATION_NOT_SUPPORTED.to_owned())
    }

    // caller must hold the mutex
    pub(crate) fn find_outbound_mapping(&mut self, o_key: &str) -> Option<&mut Mapping> {
        let mapping_life_time = self.nat_type.mapping_life_time;
        let mut expired = false;
        let (in_key, out_key) = if let Some(m) = self.outbound_map.get_mut(o_key) {
            let now = SystemTime::now();

            // check if this Mapping is expired
            if now.duration_since(m.expires).is_ok() {
                expired = true;
            } else {
                m.expires = now.add(mapping_life_time);
            }
            (
                NetworkAddressTranslator::get_inbound_map_key(m),
                NetworkAddressTranslator::get_outbound_map_key(m),
            )
        } else {
            (String::new(), String::new())
        };

        if expired {
            self.inbound_map.remove(&in_key);
            self.outbound_map.remove(&out_key);
        }

        self.outbound_map.get_mut(o_key)
    }

    // caller must hold the mutex
    pub(crate) fn find_inbound_mapping(&mut self, i_key: &str) -> Option<&Mapping> {
        let mut expired = false;
        let (in_key, out_key) = if let Some(m) = self.inbound_map.get(i_key) {
            let now = SystemTime::now();

            // check if this Mapping is expired
            if now.duration_since(m.expires).is_ok() {
                expired = true;
            }
            (
                NetworkAddressTranslator::get_inbound_map_key(m),
                NetworkAddressTranslator::get_outbound_map_key(m),
            )
        } else {
            (String::new(), String::new())
        };

        if expired {
            self.inbound_map.remove(&in_key);
            self.outbound_map.remove(&out_key);
        }

        self.inbound_map.get(i_key)
    }

    // caller must hold the mutex
    fn get_outbound_map_key(m: &Mapping) -> String {
        format!("{}:{}:{}", m.proto, m.local, m.bound)
    }

    fn get_inbound_map_key(m: &Mapping) -> String {
        format!("{}:{}", m.proto, m.mapped)
    }
}
