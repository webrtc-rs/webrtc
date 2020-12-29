use super::*;
use crate::candidate::candidate_type::*;
use crate::errors::*;
use crate::network_type::*;
use crate::url::*;

use util::Error;

use std::time::Duration;

// DEFAULT_CHECK_INTERVAL is the interval at which the agent performs candidate checks in the connecting phase
pub(crate) const DEFAULT_CHECK_INTERVAL: Duration = Duration::from_millis(200);

// keepalive_interval used to keep candidates alive
pub(crate) const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(2);

// DEFAULT_DISCONNECTED_TIMEOUT is the default time till an Agent transitions disconnected
pub(crate) const DEFAULT_DISCONNECTED_TIMEOUT: Duration = Duration::from_secs(5);

// DEFAULT_FAILED_TIMEOUT is the default time till an Agent transitions to failed after disconnected
pub(crate) const DEFAULT_FAILED_TIMEOUT: Duration = Duration::from_secs(25);

// wait time before nominating a host candidate
pub(crate) const DEFAULT_HOST_ACCEPTANCE_MIN_WAIT: Duration = Duration::from_secs(0);

// wait time before nominating a srflx candidate
pub(crate) const DEFAULT_SRFLX_ACCEPTANCE_MIN_WAIT: Duration = Duration::from_millis(500);

// wait time before nominating a prflx candidate
pub(crate) const DEFAULT_PRFLX_ACCEPTANCE_MIN_WAIT: Duration = Duration::from_millis(1000);

// wait time before nominating a relay candidate
pub(crate) const DEFAULT_RELAY_ACCEPTANCE_MIN_WAIT: Duration = Duration::from_millis(2000);

// max binding request before considering a pair failed
pub(crate) const DEFAULT_MAX_BINDING_REQUESTS: u16 = 7;

// the number of bytes that can be buffered before we start to error
pub(crate) const MAX_BUFFER_SIZE: usize = 1000 * 1000; // 1MB

// wait time before binding requests can be deleted
pub(crate) const MAX_BINDING_REQUEST_TIMEOUT: Duration = Duration::from_millis(4000);

pub(crate) fn default_candidate_types() -> Vec<CandidateType> {
    vec![
        CandidateType::Host,
        CandidateType::ServerReflexive,
        CandidateType::Relay,
    ]
}

// AgentConfig collects the arguments to ice.Agent construction into
// a single structure, for future-proofness of the interface
pub struct AgentConfig {
    urls: Vec<URL>,

    // port_min and port_max are optional. Leave them 0 for the default UDP port allocation strategy.
    port_min: u16,
    port_max: u16,

    // local_ufrag and local_pwd values used to perform connectivity
    // checks.  The values MUST be unguessable, with at least 128 bits of
    // random number generator output used to generate the password, and
    // at least 24 bits of output to generate the username fragment.
    local_ufrag: String,
    local_pwd: String,

    // MulticastDNSMode controls mDNS behavior for the ICE agent
    //TODO: MulticastDNSMode :MulticastDNSMode,

    // multicast_dnshost_name controls the hostname for this agent. If none is specified a random one will be generated
    multicast_dnshost_name: String,

    // disconnected_timeout defaults to 5 seconds when this property is nil.
    // If the duration is 0, the ICE Agent will never go to disconnected
    disconnected_timeout: Option<Duration>,

    // failed_timeout defaults to 25 seconds when this property is nil.
    // If the duration is 0, we will never go to failed.
    failed_timeout: Option<Duration>,

    // keepalive_interval determines how often should we send ICE
    // keepalives (should be less then connectiontimeout above)
    // when this is nil, it defaults to 10 seconds.
    // A keepalive interval of 0 means we never send keepalive packets
    keepalive_interval: Option<Duration>,

    // network_types is an optional configuration for disabling or enabling
    // support for specific network types.
    network_types: Vec<NetworkType>,

    // candidate_types is an optional configuration for disabling or enabling
    // support for specific candidate types.
    candidate_types: Vec<CandidateType>,

    //LoggerFactory logging.LoggerFactory

    // check_interval controls how often our internal task loop runs when
    // in the connecting state. Only useful for testing.
    check_interval: Duration,

    // max_binding_requests is the max amount of binding requests the agent will send
    // over a candidate pair for validation or nomination, if after max_binding_requests
    // the candidate is yet to answer a binding request or a nomination we set the pair as failed
    max_binding_requests: Option<u16>,

    // lite agents do not perform connectivity check and only provide host candidates.
    lite: bool,

    // nat1to1ipcandidate_type is used along with nat1to1ips to specify which candidate type
    // the 1:1 NAT IP addresses should be mapped to.
    // If unspecified or CandidateTypeHost, nat1to1ips are used to replace host candidate IPs.
    // If CandidateTypeServerReflexive, it will insert a srflx candidate (as if it was dervied
    // from a STUN server) with its port number being the one for the actual host candidate.
    // Other values will result in an error.
    nat_1to1_ip_candidate_type: CandidateType,

    // nat1to1ips contains a list of public IP addresses that are to be used as a host
    // candidate or srflx candidate. This is used typically for servers that are behind
    // 1:1 D-NAT (e.g. AWS EC2 instances) and to eliminate the need of server reflexisive
    // candidate gathering.
    nat_1to1_ips: Vec<String>,

    // host_acceptance_min_wait specify a minimum wait time before selecting host candidates
    host_acceptance_min_wait: Option<Duration>,
    // host_acceptance_min_wait specify a minimum wait time before selecting srflx candidates
    srflx_acceptance_min_wait: Option<Duration>,
    // host_acceptance_min_wait specify a minimum wait time before selecting prflx candidates
    prflx_acceptance_min_wait: Option<Duration>,
    // host_acceptance_min_wait specify a minimum wait time before selecting relay candidates
    relay_acceptance_min_wait: Option<Duration>,

    // Net is the our abstracted network interface for internal development purpose only
    // (see github.com/pion/transport/vnet)
    //TODO: Net *vnet.Net

    // interface_filter is a function that you can use in order to  whitelist or blacklist
    // the interfaces which are used to gather ICE candidates.
    interface_filter: Box<fn(String) -> bool>,

    // insecure_skip_verify controls if self-signed certificates are accepted when connecting
    // to TURN servers via TLS or DTLS
    insecure_skip_verify: bool,
    // TCPMux will be used for multiplexing incoming TCP connections for ICE TCP.
    // Currently only passive candidates are supported. This functionality is
    // experimental and the API might change in the future.
    //TODO: TCPMux TCPMux

    // Proxy Dialer is a dialer that should be implemented by the user based on golang.org/x/net/proxy
    // dial interface in order to support corporate proxies
    //TODO: ProxyDialer proxy.Dialer
}

impl AgentConfig {
    // init_with_defaults populates an agent and falls back to defaults if fields are unset
    pub(crate) fn init_with_defaults(&self, a: &mut Agent) {
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

        if self.candidate_types.is_empty() {
            a.candidate_types = default_candidate_types();
        } else {
            a.candidate_types = self.candidate_types.clone();
        }
    }

    pub(crate) fn init_ext_ip_mapping(&self, a: &mut Agent) -> Result<(), Error> {
        a.ext_ip_mapper =
            ExternalIPMapper::new(self.nat_1to1_ip_candidate_type, &self.nat_1to1_ips)?;
        if a.ext_ip_mapper.candidate_type == CandidateType::Host {
            if a.mdns_mode == MulticastDNSMode::QueryAndGather {
                return Err(ERR_MULTICAST_DNS_WITH_NAT_1TO1_IP_MAPPING.to_owned());
            }
            let mut candi_host_enabled = false;
            for candi_type in &a.candidate_types {
                if *candi_type == CandidateType::Host {
                    candi_host_enabled = true;
                    break;
                }
            }
            if !candi_host_enabled {
                return Err(ERR_INEFFECTIVE_NAT_1TO1_IP_MAPPING_HOST.to_owned());
            }
        } else if a.ext_ip_mapper.candidate_type == CandidateType::ServerReflexive {
            let mut candi_srflx_enabled = false;
            for candi_type in &a.candidate_types {
                if *candi_type == CandidateType::ServerReflexive {
                    candi_srflx_enabled = true;
                    break;
                }
            }
            if !candi_srflx_enabled {
                return Err(ERR_INEFFECTIVE_NAT_1TO1_IP_MAPPING_SRFLX.to_owned());
            }
        }

        Ok(())
    }
}
