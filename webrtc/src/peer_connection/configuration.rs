use crate::ice_transport::ice_server::RTCIceServer;
use crate::peer_connection::certificate::RTCCertificate;
use crate::peer_connection::policy::bundle_policy::RTCBundlePolicy;
use crate::peer_connection::policy::ice_transport_policy::RTCIceTransportPolicy;
use crate::peer_connection::policy::rtcp_mux_policy::RTCRtcpMuxPolicy;

/// Defines a set of parameters to configure how the peer-to-peer communication via
/// [`RTCPeerConnection`] is established or re-established. These may be set up once
/// and reused across multiple connections, and are treated as readonly. As long as
/// they are unmodified, they are safe for concurrent use.
///
/// [`RTCPeerConnection`]: crate::peer_connection::RTCPeerConnection
#[derive(Default, Clone)]
pub struct RTCConfiguration {
    /// Defines a slice describing servers available to be used by
    /// ICE, such as STUN and TURN servers.
    pub ice_servers: Vec<RTCIceServer>,

    /// Indicates which candidates the ICE Agent is allowed
    /// to use.
    pub ice_transport_policy: RTCIceTransportPolicy,

    /// Indicates which media-bundling policy to use when gathering
    /// ICE candidates.
    pub bundle_policy: RTCBundlePolicy,

    /// Indicates which rtcp-mux policy to use when gathering ICE
    /// candidates.
    pub rtcp_mux_policy: RTCRtcpMuxPolicy,

    /// **UNIMPLEMENTED:** Sets the target peer identity for the [`RTCPeerConnection`].
    /// The PeerConnection will not establish a connection to a remote peer
    /// unless it can be successfully authenticated with the provided name.
    ///
    /// [`RTCPeerConnection`]: crate::peer_connection::RTCPeerConnection
    pub peer_identity: String,

    /// A set of certificates that the [`RTCPeerConnection`] uses to authenticate.
    ///
    /// Valid values for this parameter are created through calls to the
    /// generate_certificate function.
    ///
    /// Although any given DTLS connection will use only one certificate, this
    /// attribute allows the caller to provide multiple certificates that support
    /// different algorithms. The final certificate will be selected based on the
    /// DTLS handshake, which establishes which certificates are allowed. The
    /// [`RTCPeerConnection`] implementation selects which of the certificates is
    /// used for a given connection; how certificates are selected is outside
    /// the scope of this specification.
    ///
    /// If this value is absent, then a default set of certificates is generated
    /// for each [`RTCPeerConnection`] instance.
    ///
    /// [`RTCPeerConnection`]: crate::peer_connection::RTCPeerConnection
    pub certificates: Vec<RTCCertificate>,

    /// **UNIMPLEMENTED:** The size of the prefetched ICE pool, as defined in
    /// [RFC8829](https://www.rfc-editor.org/rfc/rfc8829).
    pub ice_candidate_pool_size: u8,
}

impl RTCConfiguration {
    /// get_iceservers side-steps the strict parsing mode of the ice package
    /// (as defined in https://tools.ietf.org/html/rfc7064) by copying and then
    /// stripping any erroneous queries from "stun(s):" URLs before parsing.
    pub(crate) fn get_ice_servers(&self) -> Vec<RTCIceServer> {
        let mut ice_servers = self.ice_servers.clone();

        for ice_server in &mut ice_servers {
            for raw_url in &mut ice_server.urls {
                if raw_url.starts_with("stun") {
                    // strip the query from "stun(s):" if present
                    let parts: Vec<&str> = raw_url.split('?').collect();
                    *raw_url = parts[0].to_owned();
                }
            }
        }

        ice_servers
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_configuration_get_iceservers() {
        {
            let expected_server_str = "stun:stun.l.google.com:19302";
            let cfg = RTCConfiguration {
                ice_servers: vec![RTCIceServer {
                    urls: vec![expected_server_str.to_owned()],
                    ..Default::default()
                }],
                ..Default::default()
            };

            let parsed_urls = cfg.get_ice_servers();
            assert_eq!(parsed_urls[0].urls[0], expected_server_str);
        }

        {
            // ignore the fact that stun URLs shouldn't have a query
            let server_str = "stun:global.stun.twilio.com:3478?transport=udp";
            let expected_server_str = "stun:global.stun.twilio.com:3478";
            let cfg = RTCConfiguration {
                ice_servers: vec![RTCIceServer {
                    urls: vec![server_str.to_owned()],
                    ..Default::default()
                }],
                ..Default::default()
            };

            let parsed_urls = cfg.get_ice_servers();
            assert_eq!(parsed_urls[0].urls[0], expected_server_str);
        }
    }

    /*TODO:#[test] fn test_configuration_json() {

         let j = r#"
            {
                "iceServers": [{"URLs": ["turn:turn.example.org"],
                                "username": "jch",
                                "credential": "topsecret"
                              }],
                "iceTransportPolicy": "relay",
                "bundlePolicy": "balanced",
                "rtcpMuxPolicy": "require"
            }"#;

        conf := Configuration{
            ICEServers: []ICEServer{
                {
                    URLs:       []string{"turn:turn.example.org"},
                    Username:   "jch",
                    Credential: "topsecret",
                },
            },
            ICETransportPolicy: ICETransportPolicyRelay,
            BundlePolicy:       BundlePolicyBalanced,
            RTCPMuxPolicy:      RTCPMuxPolicyRequire,
        }

        var conf2 Configuration
        assert.NoError(t, json.Unmarshal([]byte(j), &conf2))
        assert.Equal(t, conf, conf2)

        j2, err := json.Marshal(conf2)
        assert.NoError(t, err)

        var conf3 Configuration
        assert.NoError(t, json.Unmarshal(j2, &conf3))
        assert.Equal(t, conf2, conf3)
    }*/
}
