#[cfg(test)]
mod setting_engine_test;

use crate::dtls_transport::dtls_role::DTLSRole;
use crate::ice_transport::ice_candidate_type::RTCIceCandidateType;
use dtls::extension::extension_use_srtp::SrtpProtectionProfile;
use ice::agent::agent_config::InterfaceFilterFn;
use ice::mdns::MulticastDnsMode;
use ice::network_type::NetworkType;

use crate::error::{Error, Result};

use crate::RECEIVE_MTU;
use std::sync::Arc;
use tokio::time::Duration;
use util::vnet::net::*;

#[derive(Default, Clone)]
pub struct EphemeralUDP {
    pub port_min: u16,
    pub port_max: u16,
}

#[derive(Default, Clone)]
pub struct Detach {
    pub data_channels: bool,
}

#[derive(Default, Clone)]
pub struct Timeout {
    pub ice_disconnected_timeout: Option<Duration>,
    pub ice_failed_timeout: Option<Duration>,
    pub ice_keepalive_interval: Option<Duration>,
    pub ice_host_acceptance_min_wait: Option<Duration>,
    pub ice_srflx_acceptance_min_wait: Option<Duration>,
    pub ice_prflx_acceptance_min_wait: Option<Duration>,
    pub ice_relay_acceptance_min_wait: Option<Duration>,
}

#[derive(Default, Clone)]
pub struct Candidates {
    pub ice_lite: bool,
    pub ice_network_types: Vec<NetworkType>,
    pub interface_filter: Arc<Option<InterfaceFilterFn>>,
    pub nat_1to1_ips: Vec<String>,
    pub nat_1to1_ip_candidate_type: RTCIceCandidateType,
    pub multicast_dns_mode: MulticastDnsMode,
    pub multicast_dns_host_name: String,
    pub username_fragment: String,
    pub password: String,
}

#[derive(Default, Clone)]
pub struct ReplayProtection {
    pub dtls: usize,
    pub srtp: usize,
    pub srtcp: usize,
}

/// SettingEngine allows influencing behavior in ways that are not
/// supported by the WebRTC API. This allows us to support additional
/// use-cases without deviating from the WebRTC API elsewhere.
#[derive(Default, Clone)]
pub struct SettingEngine {
    pub(crate) ephemeral_udp: EphemeralUDP,
    pub(crate) detach: Detach,
    pub(crate) timeout: Timeout,
    pub(crate) candidates: Candidates,
    pub(crate) replay_protection: ReplayProtection,
    pub(crate) sdp_media_level_fingerprints: bool,
    pub(crate) answering_dtls_role: DTLSRole,
    pub(crate) disable_certificate_fingerprint_verification: bool,
    pub(crate) disable_srtp_replay_protection: bool,
    pub(crate) disable_srtcp_replay_protection: bool,
    pub(crate) vnet: Option<Arc<Net>>,
    //BufferFactory                             :func(packetType packetio.BufferPacketType, ssrc uint32) io.ReadWriteCloser,
    //iceTCPMux                                 :ice.TCPMux,?
    //iceUDPMux                                 :ice.UDPMux,?
    //iceProxyDialer                            :proxy.Dialer,?
    pub(crate) disable_media_engine_copy: bool,
    pub(crate) srtp_protection_profiles: Vec<SrtpProtectionProfile>,
    pub(crate) receive_mtu: usize,
}

impl SettingEngine {
    /// get_receive_mtu returns the configured MTU. If SettingEngine's MTU is configured to 0 it returns the default
    pub(crate) fn get_receive_mtu(&self) -> usize {
        if self.receive_mtu != 0 {
            self.receive_mtu
        } else {
            RECEIVE_MTU
        }
    }
    /// detach_data_channels enables detaching data channels. When enabled
    /// data channels have to be detached in the OnOpen callback using the
    /// DataChannel.Detach method.
    pub fn detach_data_channels(&mut self) {
        self.detach.data_channels = true;
    }

    /// set_srtp_protection_profiles allows the user to override the default srtp Protection Profiles
    /// The default srtp protection profiles are provided by the function `defaultSrtpProtectionProfiles`
    pub fn set_srtp_protection_profiles(&mut self, profiles: Vec<SrtpProtectionProfile>) {
        self.srtp_protection_profiles = profiles
    }

    /// set_ice_timeouts sets the behavior around ICE Timeouts
    /// * disconnected_timeout is the duration without network activity before a Agent is considered disconnected. Default is 5 Seconds
    /// * failed_timeout is the duration without network activity before a Agent is considered failed after disconnected. Default is 25 Seconds
    /// * keep_alive_interval is how often the ICE Agent sends extra traffic if there is no activity, if media is flowing no traffic will be sent. Default is 2 seconds
    pub fn set_ice_timeouts(
        &mut self,
        disconnected_timeout: Option<Duration>,
        failed_timeout: Option<Duration>,
        keep_alive_interval: Option<Duration>,
    ) {
        self.timeout.ice_disconnected_timeout = disconnected_timeout;
        self.timeout.ice_failed_timeout = failed_timeout;
        self.timeout.ice_keepalive_interval = keep_alive_interval;
    }

    /// set_host_acceptance_min_wait sets the icehost_acceptance_min_wait
    pub fn set_host_acceptance_min_wait(&mut self, t: Option<Duration>) {
        self.timeout.ice_host_acceptance_min_wait = t;
    }

    /// set_srflx_acceptance_min_wait sets the icesrflx_acceptance_min_wait
    pub fn set_srflx_acceptance_min_wait(&mut self, t: Option<Duration>) {
        self.timeout.ice_srflx_acceptance_min_wait = t;
    }

    /// set_prflx_acceptance_min_wait sets the iceprflx_acceptance_min_wait
    pub fn set_prflx_acceptance_min_wait(&mut self, t: Option<Duration>) {
        self.timeout.ice_prflx_acceptance_min_wait = t;
    }

    /// set_relay_acceptance_min_wait sets the icerelay_acceptance_min_wait
    pub fn set_relay_acceptance_min_wait(&mut self, t: Option<Duration>) {
        self.timeout.ice_relay_acceptance_min_wait = t;
    }

    /// set_ephemeral_udp_port_range limits the pool of ephemeral ports that
    /// ICE UDP connections can allocate from. This affects both host candidates,
    /// and the local address of server reflexive candidates.
    pub fn set_ephemeral_udp_port_range(&mut self, port_min: u16, port_max: u16) -> Result<()> {
        if port_max < port_min {
            return Err(ice::Error::ErrPort.into());
        }

        self.ephemeral_udp.port_min = port_min;
        self.ephemeral_udp.port_max = port_max;
        Ok(())
    }

    /// set_lite configures whether or not the ice agent should be a lite agent
    pub fn set_lite(&mut self, lite: bool) {
        self.candidates.ice_lite = lite;
    }

    /// set_network_types configures what types of candidate networks are supported
    /// during local and server reflexive gathering.
    pub fn set_network_types(&mut self, candidate_types: Vec<NetworkType>) {
        self.candidates.ice_network_types = candidate_types;
    }

    /// set_interface_filter sets the filtering functions when gathering ICE candidates
    /// This can be used to exclude certain network interfaces from ICE. Which may be
    /// useful if you know a certain interface will never succeed, or if you wish to reduce
    /// the amount of information you wish to expose to the remote peer
    pub fn set_interface_filter(&mut self, filter: InterfaceFilterFn) {
        self.candidates.interface_filter = Arc::new(Some(filter));
    }

    /// set_nat_1to1_ips sets a list of external IP addresses of 1:1 (D)NAT
    /// and a candidate type for which the external IP address is used.
    /// This is useful when you are host a server using Pion on an AWS EC2 instance
    /// which has a private address, behind a 1:1 DNAT with a public IP (e.g.
    /// Elastic IP). In this case, you can give the public IP address so that
    /// Pion will use the public IP address in its candidate instead of the private
    /// IP address. The second argument, candidate_type, is used to tell Pion which
    /// type of candidate should use the given public IP address.
    /// Two types of candidates are supported:
    ///
    /// ICECandidateTypeHost:
    ///   The public IP address will be used for the host candidate in the SDP.
    /// ICECandidateTypeSrflx:
    ///   A server reflexive candidate with the given public IP address will be added
    /// to the SDP.
    ///
    /// Please note that if you choose ICECandidateTypeHost, then the private IP address
    /// won't be advertised with the peer. Also, this option cannot be used along with mDNS.
    ///
    /// If you choose ICECandidateTypeSrflx, it simply adds a server reflexive candidate
    /// with the public IP. The host candidate is still available along with mDNS
    /// capabilities unaffected. Also, you cannot give STUN server URL at the same time.
    /// It will result in an error otherwise.
    pub fn set_nat_1to1_ips(&mut self, ips: Vec<String>, candidate_type: RTCIceCandidateType) {
        self.candidates.nat_1to1_ips = ips;
        self.candidates.nat_1to1_ip_candidate_type = candidate_type;
    }

    /// set_answering_dtls_role sets the dtls_transport role that is selected when offering
    /// The dtls_transport role controls if the WebRTC Client as a client or server. This
    /// may be useful when interacting with non-compliant clients or debugging issues.
    ///
    /// DTLSRoleActive:
    ///   Act as dtls_transport Client, send the ClientHello and starts the handshake
    /// DTLSRolePassive:
    ///   Act as dtls_transport Server, wait for ClientHello
    pub fn set_answering_dtls_role(&mut self, role: DTLSRole) -> Result<()> {
        if role != DTLSRole::Client && role != DTLSRole::Server {
            return Err(Error::ErrSettingEngineSetAnsweringDTLSRole);
        }

        self.answering_dtls_role = role;
        Ok(())
    }

    /// set_vnet sets the VNet instance that is passed to ice
    /// VNet is a virtual network layer, allowing users to simulate
    /// different topologies, latency, loss and jitter. This can be useful for
    /// learning WebRTC concepts or testing your application in a lab environment
    pub fn set_vnet(&mut self, vnet: Option<Arc<Net>>) {
        self.vnet = vnet;
    }

    /// set_ice_multicast_dns_mode controls if ice queries and generates mDNS ICE Candidates
    pub fn set_ice_multicast_dns_mode(&mut self, multicast_dns_mode: ice::mdns::MulticastDnsMode) {
        self.candidates.multicast_dns_mode = multicast_dns_mode
    }

    /// set_multicast_dns_host_name sets a static HostName to be used by ice instead of generating one on startup
    /// This should only be used for a single PeerConnection. Having multiple PeerConnections with the same HostName will cause
    /// undefined behavior
    pub fn set_multicast_dns_host_name(&mut self, host_name: String) {
        self.candidates.multicast_dns_host_name = host_name;
    }

    /// set_ice_credentials sets a staic uFrag/uPwd to be used by ice
    /// This is useful if you want to do signalless WebRTC session, or having a reproducible environment with static credentials
    pub fn set_ice_credentials(&mut self, username_fragment: String, password: String) {
        self.candidates.username_fragment = username_fragment;
        self.candidates.password = password;
    }

    /// disable_certificate_fingerprint_verification disables fingerprint verification after dtls_transport Handshake has finished
    pub fn disable_certificate_fingerprint_verification(&mut self, is_disabled: bool) {
        self.disable_certificate_fingerprint_verification = is_disabled;
    }

    /// set_dtls_replay_protection_window sets a replay attack protection window size of dtls_transport connection.
    pub fn set_dtls_replay_protection_window(&mut self, n: usize) {
        self.replay_protection.dtls = n;
    }

    /// set_srtp_replay_protection_window sets a replay attack protection window size of srtp session.
    pub fn set_srtp_replay_protection_window(&mut self, n: usize) {
        self.disable_srtp_replay_protection = false;
        self.replay_protection.srtp = n;
    }

    /// set_srtcp_replay_protection_window sets a replay attack protection window size of srtcp session.
    pub fn set_srtcp_replay_protection_window(&mut self, n: usize) {
        self.disable_srtcp_replay_protection = false;
        self.replay_protection.srtcp = n;
    }

    /// disable_srtp_replay_protection disables srtp replay protection.
    pub fn disable_srtp_replay_protection(&mut self, is_disabled: bool) {
        self.disable_srtp_replay_protection = is_disabled;
    }

    /// disable_srtcp_replay_protection disables srtcp replay protection.
    pub fn disable_srtcp_replay_protection(&mut self, is_disabled: bool) {
        self.disable_srtcp_replay_protection = is_disabled;
    }

    /// set_sdp_media_level_fingerprints configures the logic for dtls_transport Fingerprint insertion
    /// If true, fingerprints will be inserted in the sdp at the fingerprint
    /// level, instead of the session level. This helps with compatibility with
    /// some webrtc implementations.
    pub fn set_sdp_media_level_fingerprints(&mut self, sdp_media_level_fingerprints: bool) {
        self.sdp_media_level_fingerprints = sdp_media_level_fingerprints;
    }

    // SetICETCPMux enables ICE-TCP when set to a non-nil value. Make sure that
    // NetworkTypeTCP4 or NetworkTypeTCP6 is enabled as well.
    //pub fn SetICETCPMux(&mut self, tcpMux ice.TCPMux) {
    //    self.iceTCPMux = tcpMux
    //}

    // SetICEUDPMux allows ICE traffic to come through a single UDP port, drastically
    // simplifying deployments where ports will need to be opened/forwarded.
    // UDPMux should be started prior to creating PeerConnections.
    //pub fn SetICEUDPMux(&mut self, udpMux ice.UDPMux) {
    //    self.iceUDPMux = udpMux
    //}

    // SetICEProxyDialer sets the proxy dialer interface based on golang.org/x/net/proxy.
    //pub fn SetICEProxyDialer(&mut self, d proxy.Dialer) {
    //    self.iceProxyDialer = d
    //}

    /// disable_media_engine_copy stops the MediaEngine from being copied. This allows a user to modify
    /// the MediaEngine after the PeerConnection has been constructed. This is useful if you wish to
    /// modify codecs after signaling. Make sure not to share MediaEngines between PeerConnections.
    pub fn disable_media_engine_copy(&mut self, is_disabled: bool) {
        self.disable_media_engine_copy = is_disabled;
    }

    /// set_receive_mtu sets the size of read buffer that copies incoming packets. This is optional.
    /// Leave this 0 for the default receive_mtu
    pub fn set_receive_mtu(&mut self, receive_mtu: usize) {
        self.receive_mtu = receive_mtu;
    }
}
