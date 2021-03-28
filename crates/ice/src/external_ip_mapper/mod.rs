#[cfg(test)]
mod external_ip_mapper_test;

use crate::candidate::*;
use crate::errors::*;

use util::Error;

use std::collections::HashMap;
use std::net::IpAddr;

pub(crate) fn validate_ip_string(ip_str: &str) -> Result<IpAddr, Error> {
    match ip_str.parse() {
        Ok(ip) => Ok(ip),
        Err(_) => Err(ERR_INVALID_NAT_1TO1_IP_MAPPING.to_owned()),
    }
}

// IpMapping holds the mapping of local and external IP address for a particular IP family
#[derive(Default, PartialEq, Debug)]
pub(crate) struct IpMapping {
    ip_sole: Option<IpAddr>, // when non-nil, this is the sole external IP for one local IP assumed
    ip_map: HashMap<String, IpAddr>, // local-to-external IP mapping (k: local, v: external)
}

impl IpMapping {
    pub(crate) fn set_sole_ip(&mut self, ip: IpAddr) -> Result<(), Error> {
        if self.ip_sole.is_some() || !self.ip_map.is_empty() {
            return Err(ERR_INVALID_NAT_1TO1_IP_MAPPING.to_owned());
        }

        self.ip_sole = Some(ip);

        Ok(())
    }

    pub(crate) fn add_ip_mapping(&mut self, loc_ip: IpAddr, ext_ip: IpAddr) -> Result<(), Error> {
        if self.ip_sole.is_some() {
            return Err(ERR_INVALID_NAT_1TO1_IP_MAPPING.to_owned());
        }

        let loc_ip_str = loc_ip.to_string();

        // check if dup of local IP
        if self.ip_map.contains_key(&loc_ip_str) {
            return Err(ERR_INVALID_NAT_1TO1_IP_MAPPING.to_owned());
        }

        self.ip_map.insert(loc_ip_str, ext_ip);

        Ok(())
    }

    pub(crate) fn find_external_ip(&self, loc_ip: IpAddr) -> Result<IpAddr, Error> {
        if let Some(ip_sole) = &self.ip_sole {
            return Ok(*ip_sole);
        }

        if let Some(ext_ip) = self.ip_map.get(&loc_ip.to_string()) {
            Ok(*ext_ip)
        } else {
            Err(ERR_EXTERNAL_MAPPED_IP_NOT_FOUND.to_owned())
        }
    }
}

#[derive(Default)]
pub(crate) struct ExternalIPMapper {
    pub(crate) ipv4_mapping: IpMapping,
    pub(crate) ipv6_mapping: IpMapping,
    pub(crate) candidate_type: CandidateType,
}

impl ExternalIPMapper {
    pub(crate) fn new(
        mut candidate_type: CandidateType,
        ips: &[String],
    ) -> Result<Option<ExternalIPMapper>, Error> {
        if ips.is_empty() {
            return Ok(None);
        }
        if candidate_type == CandidateType::Unspecified {
            candidate_type = CandidateType::Host; // defaults to host
        } else if candidate_type != CandidateType::Host
            && candidate_type != CandidateType::ServerReflexive
        {
            return Err(ERR_UNSUPPORTED_NAT_1TO1_IP_CANDIDATE_TYPE.to_owned());
        }

        let mut m = ExternalIPMapper {
            ipv4_mapping: IpMapping::default(),
            ipv6_mapping: IpMapping::default(),
            candidate_type,
        };

        for ext_ip_str in ips {
            let ip_pair: Vec<&str> = ext_ip_str.split('/').collect();
            if ip_pair.is_empty() || ip_pair.len() > 2 {
                return Err(ERR_INVALID_NAT_1TO1_IP_MAPPING.clone());
            }

            let ext_ip = validate_ip_string(ip_pair[0])?;
            if ip_pair.len() == 1 {
                if ext_ip.is_ipv4() {
                    m.ipv4_mapping.set_sole_ip(ext_ip)?;
                } else {
                    m.ipv6_mapping.set_sole_ip(ext_ip)?;
                }
            } else {
                let loc_ip = validate_ip_string(ip_pair[1])?;
                if ext_ip.is_ipv4() {
                    if !loc_ip.is_ipv4() {
                        return Err(ERR_INVALID_NAT_1TO1_IP_MAPPING.clone());
                    }

                    m.ipv4_mapping.add_ip_mapping(loc_ip, ext_ip)?;
                } else {
                    if loc_ip.is_ipv4() {
                        return Err(ERR_INVALID_NAT_1TO1_IP_MAPPING.clone());
                    }

                    m.ipv6_mapping.add_ip_mapping(loc_ip, ext_ip)?
                }
            }
        }

        Ok(Some(m))
    }

    pub(crate) fn find_external_ip(&self, local_ip_str: &str) -> Result<IpAddr, Error> {
        let loc_ip = validate_ip_string(local_ip_str)?;

        if loc_ip.is_ipv4() {
            self.ipv4_mapping.find_external_ip(loc_ip)
        } else {
            self.ipv6_mapping.find_external_ip(loc_ip)
        }
    }
}
