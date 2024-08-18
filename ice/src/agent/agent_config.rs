use std::net::IpAddr;
use std::time::Duration;

use util::vnet::net::*;

use super::*;
use crate::error::*;
use crate::mdns::*;
use crate::network_type::*;
use crate::udp_network::UDPNetwork;
use crate::url::*;

/// The interval at which the agent performs candidate checks in the connecting phase.
pub(crate) const DEFAULT_CHECK_INTERVAL: Duration = Duration::from_millis(200);

/// The interval used to keep candidates alive.
pub(crate) const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(2);

/// The default time till an Agent transitions disconnected.
pub(crate) const DEFAULT_DISCONNECTED_TIMEOUT: Duration = Duration::from_secs(5);

/// The default time till an Agent transitions to failed after disconnected.
pub(crate) const DEFAULT_FAILED_TIMEOUT: Duration = Duration::from_secs(25);

/// Wait time before nominating a host candidate.
pub(crate) const DEFAULT_HOST_ACCEPTANCE_MIN_WAIT: Duration = Duration::from_secs(0);

/// Wait time before nominating a srflx candidate.
pub(crate) const DEFAULT_SRFLX_ACCEPTANCE_MIN_WAIT: Duration = Duration::from_millis(500);

/// Wait time before nominating a prflx candidate.
pub(crate) const DEFAULT_PRFLX_ACCEPTANCE_MIN_WAIT: Duration = Duration::from_millis(1000);

/// Wait time before nominating a relay candidate.
pub(crate) const DEFAULT_RELAY_ACCEPTANCE_MIN_WAIT: Duration = Duration::from_millis(2000);

/// Max binding request before considering a pair failed.
pub(crate) const DEFAULT_MAX_BINDING_REQUESTS: u16 = 7;

/// The number of bytes that can be buffered before we start to error.
pub(crate) const MAX_BUFFER_SIZE: usize = 1000 * 1000; // 1MB

/// Wait time before binding requests can be deleted.
pub(crate) const MAX_BINDING_REQUEST_TIMEOUT: Duration = Duration::from_millis(4000);

pub(crate) fn default_candidate_types() -> Vec<CandidateType> {
    vec![
        CandidateType::Host,
        CandidateType::ServerReflexive,
        CandidateType::Relay,
    ]
}

pub type InterfaceFilterFn = Box<dyn (Fn(&str) -> bool) + Send + Sync>;
pub type IpFilterFn = Box<dyn (Fn(IpAddr) -> bool) + Send + Sync>;

/// Collects the arguments to `ice::Agent` construction into a single structure, for
/// future-proofness of the interface.
#[derive(Default)]
pub struct AgentConfig {
    pub urls: Vec<Url>,

    /// Controls how the UDP network stack works.
    /// See [`UDPNetwork`]
    pub udp_network: UDPNetwork,

    /// It is used to perform connectivity checks. The values MUST be unguessable, with at least
    /// 128 bits of random number generator output used to generate the password, and at least 24
    /// bits of output to generate the username fragment.
    pub local_ufrag: String,
    /// It is used to perform connectivity checks. The values MUST be unguessable, with at least
    /// 128 bits of random number generator output used to generate the password, and at least 24
    /// bits of output to generate the username fragment.
    pub local_pwd: String,

    /// Controls mDNS behavior for the ICE agent.
    pub multicast_dns_mode: MulticastDnsMode,

    /// Controls the hostname for this agent. If none is specified a random one will be generated.
    pub multicast_dns_host_name: String,

    /// Control mDNS destination address
    pub multicast_dns_dest_addr: String,

    /// Defaults to 5 seconds when this property is nil.
    /// If the duration is 0, the ICE Agent will never go to disconnected.
    pub disconnected_timeout: Option<Duration>,

    /// Defaults to 25 seconds when this property is nil.
    /// If the duration is 0, we will never go to failed.
    pub failed_timeout: Option<Duration>,

    /// Determines how often should we send ICE keepalives (should be less then connectiontimeout
    /// above) when this is nil, it defaults to 10 seconds.
    /// A keepalive interval of 0 means we never send keepalive packets
    pub keepalive_interval: Option<Duration>,

    /// An optional configuration for disabling or enabling support for specific network types.
    pub network_types: Vec<NetworkType>,

    /// An optional configuration for disabling or enabling support for specific candidate types.
    pub candidate_types: Vec<CandidateType>,

    //LoggerFactory logging.LoggerFactory
    /// Controls how often our internal task loop runs when in the connecting state.
    /// Only useful for testing.
    pub check_interval: Duration,

    /// The max amount of binding requests the agent will send over a candidate pair for validation
    /// or nomination, if after max_binding_requests the candidate is yet to answer a binding
    /// request or a nomination we set the pair as failed.
    pub max_binding_requests: Option<u16>,

    pub is_controlling: bool,

    /// lite agents do not perform connectivity check and only provide host candidates.
    pub lite: bool,

    /// It is used along with nat1to1ips to specify which candidate type the 1:1 NAT IP addresses
    /// should be mapped to. If unspecified or CandidateTypeHost, nat1to1ips are used to replace
    /// host candidate IPs. If CandidateTypeServerReflexive, it will insert a srflx candidate (as
    /// if it was derived from a STUN server) with its port number being the one for the actual host
    /// candidate.  Other values will result in an error.
    pub nat_1to1_ip_candidate_type: CandidateType,

    /// Contains a list of public IP addresses that are to be used as a host candidate or srflx
    /// candidate. This is used typically for servers that are behind 1:1 D-NAT (e.g. AWS EC2
    /// instances) and to eliminate the need of server reflexisive candidate gathering.
    pub nat_1to1_ips: Vec<String>,

    /// Specify a minimum wait time before selecting host candidates.
    pub host_acceptance_min_wait: Option<Duration>,
    /// Specify a minimum wait time before selecting srflx candidates.
    pub srflx_acceptance_min_wait: Option<Duration>,
    /// Specify a minimum wait time before selecting prflx candidates.
    pub prflx_acceptance_min_wait: Option<Duration>,
    /// Specify a minimum wait time before selecting relay candidates.
    pub relay_acceptance_min_wait: Option<Duration>,

    /// Net is the our abstracted network interface for internal development purpose only
    /// (see (github.com/pion/transport/vnet)[github.com/pion/transport/vnet]).
    pub net: Option<Arc<Net>>,

    /// A function that you can use in order to whitelist or blacklist the interfaces which are
    /// used to gather ICE candidates.
    pub interface_filter: Arc<Option<InterfaceFilterFn>>,

    /// A function that you can use in order to whitelist or blacklist
    /// the ips which are used to gather ICE candidates.
    pub ip_filter: Arc<Option<IpFilterFn>>,

    /// Controls if self-signed certificates are accepted when connecting to TURN servers via TLS or
    /// DTLS.
    pub insecure_skip_verify: bool,

    /// Include loopback addresses in the candidate list.
    pub include_loopback: bool,
}

impl AgentConfig {
    /// Populates an agent and falls back to defaults if fields are unset.
    pub(crate) fn init_with_defaults(&self, a: &mut AgentInternal) {
        if let Some(max_binding_requests) = self.max_binding_requests {
            a.max_binding_requests = max_binding_requests;
        } else {
            a.max_binding_requests = DEFAULT_MAX_BINDING_REQUESTS;
        }

        if let Some(host_acceptance_min_wait) = self.host_acceptance_min_wait {
            a.host_acceptance_min_wait = host_acceptance_min_wait;
        } else {
            a.host_acceptance_min_wait = DEFAULT_HOST_ACCEPTANCE_MIN_WAIT;
        }

        if let Some(srflx_acceptance_min_wait) = self.srflx_acceptance_min_wait {
            a.srflx_acceptance_min_wait = srflx_acceptance_min_wait;
        } else {
            a.srflx_acceptance_min_wait = DEFAULT_SRFLX_ACCEPTANCE_MIN_WAIT;
        }

        if let Some(prflx_acceptance_min_wait) = self.prflx_acceptance_min_wait {
            a.prflx_acceptance_min_wait = prflx_acceptance_min_wait;
        } else {
            a.prflx_acceptance_min_wait = DEFAULT_PRFLX_ACCEPTANCE_MIN_WAIT;
        }

        if let Some(relay_acceptance_min_wait) = self.relay_acceptance_min_wait {
            a.relay_acceptance_min_wait = relay_acceptance_min_wait;
        } else {
            a.relay_acceptance_min_wait = DEFAULT_RELAY_ACCEPTANCE_MIN_WAIT;
        }

        if let Some(disconnected_timeout) = self.disconnected_timeout {
            a.disconnected_timeout = disconnected_timeout;
        } else {
            a.disconnected_timeout = DEFAULT_DISCONNECTED_TIMEOUT;
        }

        if let Some(failed_timeout) = self.failed_timeout {
            a.failed_timeout = failed_timeout;
        } else {
            a.failed_timeout = DEFAULT_FAILED_TIMEOUT;
        }

        if let Some(keepalive_interval) = self.keepalive_interval {
            a.keepalive_interval = keepalive_interval;
        } else {
            a.keepalive_interval = DEFAULT_KEEPALIVE_INTERVAL;
        }

        if self.check_interval == Duration::from_secs(0) {
            a.check_interval = DEFAULT_CHECK_INTERVAL;
        } else {
            a.check_interval = self.check_interval;
        }
    }

    pub(crate) fn init_ext_ip_mapping(
        &self,
        mdns_mode: MulticastDnsMode,
        candidate_types: &[CandidateType],
    ) -> Result<Option<ExternalIpMapper>> {
        if let Some(ext_ip_mapper) =
            ExternalIpMapper::new(self.nat_1to1_ip_candidate_type, &self.nat_1to1_ips)?
        {
            if ext_ip_mapper.candidate_type == CandidateType::Host {
                if mdns_mode == MulticastDnsMode::QueryAndGather {
                    return Err(Error::ErrMulticastDnsWithNat1to1IpMapping);
                }
                let mut candi_host_enabled = false;
                for candi_type in candidate_types {
                    if *candi_type == CandidateType::Host {
                        candi_host_enabled = true;
                        break;
                    }
                }
                if !candi_host_enabled {
                    return Err(Error::ErrIneffectiveNat1to1IpMappingHost);
                }
            } else if ext_ip_mapper.candidate_type == CandidateType::ServerReflexive {
                let mut candi_srflx_enabled = false;
                for candi_type in candidate_types {
                    if *candi_type == CandidateType::ServerReflexive {
                        candi_srflx_enabled = true;
                        break;
                    }
                }
                if !candi_srflx_enabled {
                    return Err(Error::ErrIneffectiveNat1to1IpMappingSrflx);
                }
            }

            Ok(Some(ext_ip_mapper))
        } else {
            Ok(None)
        }
    }
}
