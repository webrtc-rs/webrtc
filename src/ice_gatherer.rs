//! ICE Candidate Gathering (Sans-I/O)
//!
//! This module provides RTCIceGatherer for gathering ICE candidates in a Sans-I/O manner.
//! Unlike the old async version, this gatherer is a configuration object that holds
//! the ICE servers and state.

use crate::peer_connection::InnerMessage;
use crate::runtime::{Runtime, Sender};
use rtc::peer_connection::configuration::{RTCIceServer, RTCIceTransportPolicy};
use rtc::peer_connection::transport::RTCIceCandidateInit;
use std::net::SocketAddr;
use std::sync::Arc;

/// ICEGatherOptions provides options relating to the gathering of ICE candidates.
#[derive(Default, Debug, Clone)]
pub struct RTCIceGatherOptions {
    pub ice_servers: Vec<RTCIceServer>,
    pub ice_gather_policy: RTCIceTransportPolicy,
}

/// ICE Gatherer state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RTCIceGathererState {
    /// Gatherer is new and hasn't started gathering
    New,
    /// Gatherer is actively gathering candidates
    Gathering,
    /// Gathering is complete
    Complete,
}

/// RTCIceGatherer gathers local host, server reflexive and relay candidates
/// in a Sans-I/O manner.
///
/// This is a Sans-I/O configuration object that holds ICE servers and gathering state.
pub struct RTCIceGatherer {
    msg_tx: Sender<InnerMessage>,
    ice_servers: Vec<RTCIceServer>,
    gather_policy: RTCIceTransportPolicy,
    state: RTCIceGathererState,
}

impl RTCIceGatherer {
    /// Create a new ICE gatherer with ICE servers and gather policy
    pub(crate) fn new(outgoing_tx: Sender<InnerMessage>, opts: RTCIceGatherOptions) -> Self {
        Self {
            msg_tx: outgoing_tx,
            ice_servers: opts.ice_servers,
            gather_policy: opts.ice_gather_policy,
            state: RTCIceGathererState::New,
        }
    }

    /// Get the current gathering state
    pub fn state(&self) -> RTCIceGathererState {
        self.state
    }

    /// Get ICE servers configured for this gatherer
    pub fn ice_servers(&self) -> &[RTCIceServer] {
        &self.ice_servers
    }

    /// Get the ICE transport policy
    pub fn gather_policy(&self) -> RTCIceTransportPolicy {
        self.gather_policy
    }

    /// Mark gathering as started
    pub(crate) fn set_gathering(&mut self) {
        self.state = RTCIceGathererState::Gathering;
    }

    /// Mark gathering as complete
    pub(crate) fn set_complete(&mut self) {
        self.state = RTCIceGathererState::Complete;
    }

    pub(crate) async fn gather(&self, runtime: Arc<dyn Runtime>, local_addr: SocketAddr) {
        // Trigger ICE candidate gathering

        // Gather host candidates (synchronous)
        for host_candidate in RTCIceGatherer::gather_host_candidates(local_addr) {
            if let Err(e) = self
                .msg_tx
                .send(InnerMessage::LocalIceCandidate(host_candidate))
                .await
            {
                log::warn!("Failed to send host candidate: {}", e);
            }
        }

        if !self.ice_servers.is_empty() {
            let ice_servers = self.ice_servers.clone();
            let outgoing_tx = self.msg_tx.clone();
            let runtime_cloned = runtime.clone();
            runtime.spawn(Box::pin(async move {
                // Spawn background task for STUN gathering (server reflexive candidates)
                for srflx_candidate in
                    RTCIceGatherer::gather_srflx_candidates(runtime_cloned, local_addr, ice_servers)
                        .await
                {
                    if let Err(e) = outgoing_tx
                        .send(InnerMessage::LocalIceCandidate(srflx_candidate))
                        .await
                    {
                        log::warn!("Failed to send SRFLX candidate: {}", e);
                    }
                }
            }));
        }
    }

    /// Gather host ICE candidates from a local socket address
    ///
    /// This is a pure function that creates host candidates without performing I/O.
    fn gather_host_candidates(local_addr: SocketAddr) -> Vec<RTCIceCandidateInit> {
        let mut candidates = Vec::new();

        // Create a simple host candidate string
        // Format: candidate:<foundation> <component> <protocol> <priority> <address> <port> typ host
        let candidate_string = format!(
            "candidate:1 1 UDP 2130706431 {} {} typ host",
            local_addr.ip(),
            local_addr.port()
        );

        let candidate_init = RTCIceCandidateInit {
            candidate: candidate_string,
            sdp_mid: Some("0".to_string()),
            sdp_mline_index: Some(0),
            username_fragment: None,
            url: None,
        };

        candidates.push(candidate_init);
        candidates
    }

    /// Gather server reflexive (srflx) ICE candidates via STUN
    ///
    /// This performs actual I/O to query STUN servers and should be called
    /// in an async context.
    async fn gather_srflx_candidates(
        runtime: Arc<dyn Runtime>,
        local_addr: SocketAddr,
        ice_servers: Vec<RTCIceServer>,
    ) -> Vec<RTCIceCandidateInit> {
        let mut candidates = Vec::new();

        for ice_server in ice_servers {
            for url in &ice_server.urls {
                // Only handle stun: URLs for now
                if !url.starts_with("stun:") {
                    continue;
                }

                match RTCIceGatherer::gather_from_stun_server(runtime.clone(), local_addr, url)
                    .await
                {
                    Ok(candidate) => {
                        candidates.push(candidate);
                    }
                    Err(e) => {
                        log::warn!("Failed to gather srflx candidate from {}: {}", url, e);
                    }
                }
            }
        }

        candidates
    }

    /// Gather a single srflx candidate from a STUN server
    async fn gather_from_stun_server(
        runtime: Arc<dyn Runtime>,
        local_addr: SocketAddr,
        stun_url: &str,
    ) -> Result<RTCIceCandidateInit, Box<dyn std::error::Error + Send + Sync>> {
        use crate::runtime;
        use bytes::BytesMut;
        use rtc::ice::candidate::CandidateConfig;
        use rtc::ice::candidate::candidate_server_reflexive::CandidateServerReflexiveConfig;
        use rtc::sansio::Protocol;
        use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
        use rtc::stun::client::ClientBuilder;
        use rtc::stun::message::*;
        use rtc::stun::xoraddr::XorMappedAddress;
        use std::time::{Duration, Instant};

        // Resolve STUN server address (add default port 3478 if not specified)
        let stun_server_addr_str = if stun_url.contains(':') {
            stun_url
                .strip_prefix("stun:")
                .unwrap_or(stun_url)
                .to_string()
        } else {
            format!(
                "{}:3478",
                stun_url.strip_prefix("stun:").unwrap_or(stun_url)
            )
        };

        log::debug!("Resolving STUN server: {}", stun_server_addr_str);

        // Resolve hostname to IP address using runtime-agnostic helper
        let stun_server_addr: SocketAddr = runtime::resolve_host(&stun_server_addr_str)
            .await?
            .into_iter()
            .next()
            .ok_or("Failed to resolve STUN server hostname")?;

        log::debug!(
            "Resolved STUN server {} to {}",
            stun_server_addr_str,
            stun_server_addr
        );

        // Create a temporary UDP socket for STUN (match IP version of STUN server)
        let addr = if stun_server_addr.is_ipv6() {
            "[::]:0"
        } else {
            "0.0.0.0:0"
        };
        let socket = std::net::UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;

        let stun_socket = runtime.wrap_udp_socket(socket)?;
        let stun_local_addr = stun_socket.local_addr()?;

        log::debug!("STUN client bound to {}", stun_local_addr);

        // Create STUN client using the sans-I/O pattern
        let transport_context = TransportContext::default();
        let mut client = ClientBuilder::new().build(
            stun_local_addr,
            transport_context.peer_addr,
            TransportProtocol::UDP,
        )?;

        // Create STUN binding request
        let mut msg = Message::new();
        msg.build(&[Box::<TransactionId>::default(), Box::new(BINDING_REQUEST)])?;

        // Send the request
        client.handle_write(msg)?;

        while let Some(transmit) = client.poll_write() {
            stun_socket
                .send_to(&transmit.message, stun_server_addr)
                .await?;
            log::debug!("Sent STUN binding request to {}", stun_server_addr);
        }

        // Wait for response with timeout
        let xor_addr = runtime::timeout(Duration::from_secs(5), async {
            let mut buf = vec![0u8; 1500];
            let (n, peer_addr) = stun_socket.recv_from(&mut buf).await?;

            log::debug!("Received {} bytes from {}", n, peer_addr);

            // Feed response to client
            client.handle_read(TaggedBytesMut {
                now: Instant::now(),
                transport: TransportContext {
                    local_addr: stun_local_addr,
                    peer_addr,
                    transport_protocol: TransportProtocol::UDP,
                    ecn: None,
                },
                message: BytesMut::from(&buf[..n]),
            })?;

            // Poll for event
            if let Some(event) = client.poll_event() {
                let response_msg = event.result?;
                let mut xor_addr = XorMappedAddress::default();
                xor_addr.get_from(&response_msg)?;
                log::info!("Got STUN response: {}:{}", xor_addr.ip, xor_addr.port);
                Ok::<XorMappedAddress, Box<dyn std::error::Error + Send + Sync>>(xor_addr)
            } else {
                Err("No STUN response event".into())
            }
        })
        .await
        .map_err(|_| "STUN request timeout")??;

        // Close the STUN client
        client.close()?;

        // Create server reflexive candidate
        let candidate = CandidateServerReflexiveConfig {
            base_config: CandidateConfig {
                network: "udp".to_owned(),
                address: xor_addr.ip.to_string(),
                port: xor_addr.port,
                component: 1,
                ..Default::default()
            },
            rel_addr: local_addr.ip().to_string(),
            rel_port: local_addr.port(),
            ..Default::default()
        }
        .new_candidate_server_reflexive()?;

        let mut candidate_init =
            rtc::peer_connection::transport::RTCIceCandidate::from(&candidate).to_json()?;
        candidate_init.url = Some(stun_url.to_string());

        log::info!("Generated srflx candidate: {}", candidate_init.candidate);
        Ok(candidate_init)
    }
}
