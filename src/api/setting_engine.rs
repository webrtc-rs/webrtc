use ice::mdns::MulticastDnsMode;
use ice::network_type::NetworkType;

use crate::dtls::dtls_role::DTLSRole;
use crate::ice::ice_candidate::ice_candidate_type::ICECandidateType;
use dtls::extension::extension_use_srtp::SrtpProtectionProfile;
use ice::agent::agent_config::InterfaceFilterFn;
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
    pub ice_disconnected_timeout: Duration,
    pub ice_failed_timeout: Duration,
    pub ice_keepalive_interval: Duration,
    pub ice_host_acceptance_min_wait: Duration,
    pub ice_srflx_acceptance_min_wait: Duration,
    pub ice_prflx_acceptance_min_wait: Duration,
    pub ice_relay_acceptance_min_wait: Duration,
}

#[derive(Default, Clone)]
pub struct Candidates {
    pub ice_lite: bool,
    pub ice_network_types: Vec<NetworkType>,
    pub interface_filter: Arc<Option<InterfaceFilterFn>>,
    pub nat_1to1_ips: Vec<String>,
    pub nat_1to1_ip_candidate_type: ICECandidateType,
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
    pub(crate) net: Option<Arc<Net>>,
    //TODO: BufferFactory                             :func(packetType packetio.BufferPacketType, ssrc uint32) io.ReadWriteCloser,
    //TODO:? iceTCPMux                                 :ice.TCPMux,?
    //iceUDPMux                                 :ice.UDPMux,?
    //iceProxyDialer                            :proxy.Dialer,?
    pub(crate) disable_media_engine_copy: bool,
    pub(crate) srtp_protection_profiles: Vec<SrtpProtectionProfile>,
}

impl SettingEngine {
    /*TODO:
    // DetachDataChannels enables detaching data channels. When enabled
    // data channels have to be detached in the OnOpen callback using the
    // DataChannel.Detach method.
    func (e *SettingEngine) DetachDataChannels() {
        e.Detach.data_channels = true
    }

    // SetSRTPProtectionProfiles allows the user to override the default srtp Protection Profiles
    // The default srtp protection profiles are provided by the function `defaultSrtpProtectionProfiles`
    func (e *SettingEngine) SetSRTPProtectionProfiles(profiles ...dtls.SRTPProtectionProfile) {
        e.srtp_protection_profiles = profiles
    }

    // SetICETimeouts sets the behavior around ICE Timeouts
    // * disconnectedTimeout is the duration without network activity before a Agent is considered disconnected. Default is 5 Seconds
    // * failedTimeout is the duration without network activity before a Agent is considered failed after disconnected. Default is 25 Seconds
    // * keepAliveInterval is how often the ICE Agent sends extra traffic if there is no activity, if media is flowing no traffic will be sent. Default is 2 seconds
    func (e *SettingEngine) SetICETimeouts(disconnectedTimeout, failedTimeout, keepAliveInterval time.Duration) {
        e.timeout.icedisconnected_timeout = &disconnectedTimeout
        e.timeout.icefailed_timeout = &failedTimeout
        e.timeout.icekeepalive_interval = &keepAliveInterval
    }

    // SetHostAcceptanceMinWait sets the icehost_acceptance_min_wait
    func (e *SettingEngine) SetHostAcceptanceMinWait(t time.Duration) {
        e.timeout.icehost_acceptance_min_wait = &t
    }

    // SetSrflxAcceptanceMinWait sets the icesrflx_acceptance_min_wait
    func (e *SettingEngine) SetSrflxAcceptanceMinWait(t time.Duration) {
        e.timeout.icesrflx_acceptance_min_wait = &t
    }

    // SetPrflxAcceptanceMinWait sets the iceprflx_acceptance_min_wait
    func (e *SettingEngine) SetPrflxAcceptanceMinWait(t time.Duration) {
        e.timeout.iceprflx_acceptance_min_wait = &t
    }

    // SetRelayAcceptanceMinWait sets the icerelay_acceptance_min_wait
    func (e *SettingEngine) SetRelayAcceptanceMinWait(t time.Duration) {
        e.timeout.icerelay_acceptance_min_wait = &t
    }

    // SetEphemeralUDPPortRange limits the pool of ephemeral ports that
    // ICE UDP connections can allocate from. This affects both host candidates,
    // and the local address of server reflexive candidates.
    func (e *SettingEngine) SetEphemeralUDPPortRange(portMin, portMax uint16) error {
        if portMax < portMin {
            return ice.ErrPort
        }

        e.ephemeralUDP.port_min = portMin
        e.ephemeralUDP.port_max = portMax
        return nil
    }

    // SetLite configures whether or not the ice agent should be a lite agent
    func (e *SettingEngine) SetLite(lite bool) {
        e.candidates.icelite = lite
    }

    // SetNetworkTypes configures what types of candidate networks are supported
    // during local and server reflexive gathering.
    func (e *SettingEngine) SetNetworkTypes(candidateTypes []NetworkType) {
        e.candidates.icenetwork_types = candidateTypes
    }

    // SetInterfaceFilter sets the filtering functions when gathering ICE candidates
    // This can be used to exclude certain network interfaces from ICE. Which may be
    // useful if you know a certain interface will never succeed, or if you wish to reduce
    // the amount of information you wish to expose to the remote peer
    func (e *SettingEngine) SetInterfaceFilter(filter func(string) bool) {
        e.candidates.interface_filter = filter
    }

    // SetNAT1To1IPs sets a list of external IP addresses of 1:1 (D)NAT
    // and a candidate type for which the external IP address is used.
    // This is useful when you are host a server using Pion on an AWS EC2 instance
    // which has a private address, behind a 1:1 DNAT with a public IP (e.g.
    // Elastic IP). In this case, you can give the public IP address so that
    // Pion will use the public IP address in its candidate instead of the private
    // IP address. The second argument, candidateType, is used to tell Pion which
    // type of candidate should use the given public IP address.
    // Two types of candidates are supported:
    //
    // ICECandidateTypeHost:
    //		The public IP address will be used for the host candidate in the SDP.
    // ICECandidateTypeSrflx:
    //		A server reflexive candidate with the given public IP address will be added
    // to the SDP.
    //
    // Please note that if you choose ICECandidateTypeHost, then the private IP address
    // won't be advertised with the peer. Also, this option cannot be used along with mDNS.
    //
    // If you choose ICECandidateTypeSrflx, it simply adds a server reflexive candidate
    // with the public IP. The host candidate is still available along with mDNS
    // capabilities unaffected. Also, you cannot give STUN server URL at the same time.
    // It will result in an error otherwise.
    func (e *SettingEngine) SetNAT1To1IPs(ips []string, candidateType ICECandidateType) {
        e.candidates.nat1to1ips = ips
        e.candidates.nat1to1ipcandidate_type = candidateType
    }

    // SetAnsweringDTLSRole sets the dtls role that is selected when offering
    // The dtls role controls if the WebRTC Client as a client or server. This
    // may be useful when interacting with non-compliant clients or debugging issues.
    //
    // DTLSRoleActive:
    // 		Act as dtls Client, send the ClientHello and starts the handshake
    // DTLSRolePassive:
    // 		Act as dtls Server, wait for ClientHello
    func (e *SettingEngine) SetAnsweringDTLSRole(role DTLSRole) error {
        if role != DTLSRoleClient && role != DTLSRoleServer {
            return errSettingEngineSetAnsweringDTLSRole
        }

        e.answering_dtlsrole = role
        return nil
    }

    // SetVNet sets the VNet instance that is passed to pion/ice
    //
    // VNet is a virtual network layer for Pion, allowing users to simulate
    // different topologies, latency, loss and jitter. This can be useful for
    // learning WebRTC concepts or testing your application in a lab environment
    func (e *SettingEngine) SetVNet(vnet *vnet.Net) {
        e.vnet = vnet
    }
    */
    /// set_ice_multicast_dns_mode controls if pion/ice queries and generates mDNS ICE Candidates
    pub fn set_ice_multicast_dns_mode(&mut self, multicast_dns_mode: ice::mdns::MulticastDnsMode) {
        self.candidates.multicast_dns_mode = multicast_dns_mode
    }
    /*
    // SetMulticastDNSHostName sets a static HostName to be used by pion/ice instead of generating one on startup
    //
    // This should only be used for a single PeerConnection. Having multiple PeerConnections with the same HostName will cause
    // undefined behavior
    func (e *SettingEngine) SetMulticastDNSHostName(hostName string) {
        e.candidates.multicast_dnshost_name = hostName
    }

    // SetICECredentials sets a staic uFrag/uPwd to be used by pion/ice
    //
    // This is useful if you want to do signalless WebRTC session, or having a reproducible environment with static credentials
    func (e *SettingEngine) SetICECredentials(usernameFragment, password string) {
        e.candidates.username_fragment = usernameFragment
        e.candidates.password = password
    }

    // DisableCertificateFingerprintVerification disables fingerprint verification after dtls Handshake has finished
    func (e *SettingEngine) DisableCertificateFingerprintVerification(isDisabled bool) {
        e.disable_certificate_fingerprint_verification = isDisabled
    }

    // SetDTLSReplayProtectionWindow sets a replay attack protection window size of dtls connection.
    func (e *SettingEngine) SetDTLSReplayProtectionWindow(n uint) {
        e.replayProtection.dtls = &n
    }

    // SetSRTPReplayProtectionWindow sets a replay attack protection window size of srtp session.
    func (e *SettingEngine) SetSRTPReplayProtectionWindow(n uint) {
        e.disable_srtpreplay_protection = false
        e.replayProtection.srtp = &n
    }

    // SetSRTCPReplayProtectionWindow sets a replay attack protection window size of srtcp session.
    func (e *SettingEngine) SetSRTCPReplayProtectionWindow(n uint) {
        e.disable_srtcpreplay_protection = false
        e.replayProtection.srtcp = &n
    }

    // DisableSRTPReplayProtection disables srtp replay protection.
    func (e *SettingEngine) DisableSRTPReplayProtection(isDisabled bool) {
        e.disable_srtpreplay_protection = isDisabled
    }

    // DisableSRTCPReplayProtection disables srtcp replay protection.
    func (e *SettingEngine) DisableSRTCPReplayProtection(isDisabled bool) {
        e.disable_srtcpreplay_protection = isDisabled
    }

    // SetSDPMediaLevelFingerprints configures the logic for dtls Fingerprint insertion
    // If true, fingerprints will be inserted in the sdp at the fingerprint
    // level, instead of the session level. This helps with compatibility with
    // some webrtc implementations.
    func (e *SettingEngine) SetSDPMediaLevelFingerprints(sdp_media_level_fingerprints bool) {
        e.sdp_media_level_fingerprints = sdp_media_level_fingerprints
    }

    // SetICETCPMux enables ICE-TCP when set to a non-nil value. Make sure that
    // NetworkTypeTCP4 or NetworkTypeTCP6 is enabled as well.
    func (e *SettingEngine) SetICETCPMux(tcpMux ice.TCPMux) {
        e.iceTCPMux = tcpMux
    }

    // SetICEUDPMux allows ICE traffic to come through a single UDP port, drastically
    // simplifying deployments where ports will need to be opened/forwarded.
    // UDPMux should be started prior to creating PeerConnections.
    func (e *SettingEngine) SetICEUDPMux(udpMux ice.UDPMux) {
        e.iceUDPMux = udpMux
    }

    // SetICEProxyDialer sets the proxy dialer interface based on golang.org/x/net/proxy.
    func (e *SettingEngine) SetICEProxyDialer(d proxy.Dialer) {
        e.iceProxyDialer = d
    }

    // DisableMediaEngineCopy stops the MediaEngine from being copied. This allows a user to modify
    // the MediaEngine after the PeerConnection has been constructed. This is useful if you wish to
    // modify codecs after signaling. Make sure not to share MediaEngines between PeerConnections.
    func (e *SettingEngine) DisableMediaEngineCopy(isDisabled bool) {
        e.disable_media_engine_copy = isDisabled
    }
    */
}
