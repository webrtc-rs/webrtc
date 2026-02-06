//! ICE Candidate Gathering (Sans-I/O)
//!
//! This module provides RTCIceGatherer for gathering ICE candidates in a Sans-I/O manner.
//! Unlike the old async version, this gatherer is a configuration object that holds
//! the ICE servers and state.

use crate::peer_connection::MessageInner;
use crate::runtime::{AsyncUdpSocket, JoinHandle, Mutex, Runtime, Sender};
use crate::{Error, Result};
use log::error;
use rtc::ice::candidate::CandidateConfig;
use rtc::peer_connection::configuration::{RTCIceServer, RTCIceTransportPolicy};
use rtc::peer_connection::transport::{CandidateHostConfig, RTCIceCandidateInit};
use std::net::SocketAddr;
use std::sync::Arc;

/// ICEGatherOptions provides options relating to the gathering of ICE candidates.
#[derive(Default, Debug, Clone)]
pub struct RTCIceGatherOptions {
    pub ice_servers: Vec<RTCIceServer>,
    pub ice_gather_policy: RTCIceTransportPolicy,
}

/// RTCIceGatherer gathers local host, server reflexive and relay candidates
/// in a Sans-I/O manner.
///
/// This is a Sans-I/O configuration object that holds ICE servers and gathering state.
pub struct RTCIceGatherer {
    runtime: Arc<dyn Runtime>,
    msg_tx: Sender<MessageInner>,
    sockets: Vec<Arc<dyn AsyncUdpSocket>>,
    ice_servers: Vec<RTCIceServer>,
    gather_policy: RTCIceTransportPolicy,
    join_handle: Mutex<Option<JoinHandle>>,
}

impl RTCIceGatherer {
    /// Create a new ICE gatherer with ICE servers and gather policy
    pub(crate) fn new(
        runtime: Arc<dyn Runtime>,
        msg_tx: Sender<MessageInner>,
        sockets: Vec<Arc<dyn AsyncUdpSocket>>,
        opts: RTCIceGatherOptions,
    ) -> Self {
        Self {
            runtime,
            msg_tx,
            sockets,
            ice_servers: opts.ice_servers,
            gather_policy: opts.ice_gather_policy,
            join_handle: Mutex::new(None),
        }
    }

    pub(crate) async fn gather(&self) -> Result<()> {
        {
            let mut join_handle = self.join_handle.lock().await;
            if let Some(join_handle) = join_handle.take() {
                join_handle.abort();
            }
        }

        // Gather host candidates (synchronous)
        if let Err(err) = self.gather_host_candidates().await {
            error!("Error gathering host candidates: {}", err);
        }

        {
            let runtime = self.runtime.clone();
            let sockets = self.sockets.clone();
            let ice_servers = self.ice_servers.clone();
            let msg_tx = self.msg_tx.clone();

            let handle = self.runtime.spawn(Box::pin(async move {
                // Spawn background task for STUN gathering (server reflexive candidates)
                if let Err(err) = RTCIceGatherer::gather_srflx_candidates(
                    runtime,
                    sockets,
                    ice_servers,
                    msg_tx.clone(),
                )
                .await
                {
                    error!("Error gathering srflx candidates: {}", err);
                }

                if let Err(err) = msg_tx.send(MessageInner::IceGatheringComplete).await {
                    error!("Error sending IceGatheringComplete: {}", err);
                }
            }));

            let mut join_handle = self.join_handle.lock().await;
            *join_handle = Some(handle);
        }

        Ok(())
    }

    /// Gather host ICE candidates from a local socket address
    ///
    /// This is a pure function that creates host candidates without performing I/O.
    async fn gather_host_candidates(&self) -> Result<()> {
        for socket in &self.sockets {
            let local_addr = socket.local_addr()?;

            let candidate = CandidateHostConfig {
                base_config: CandidateConfig {
                    network: "udp".to_owned(),
                    address: local_addr.ip().to_string(),
                    port: local_addr.port(),
                    component: 1,
                    ..Default::default()
                },
                ..Default::default()
            }
            .new_candidate_host()?;

            let candidate_init =
                rtc::peer_connection::transport::RTCIceCandidate::from(&candidate).to_json()?;

            self.msg_tx
                .send(MessageInner::LocalIceCandidate(candidate_init))
                .await
                .map_err(|e| Error::Other(e.to_string()))?;
        }

        Ok(())
    }

    /// Gather server reflexive (srflx) ICE candidates via STUN
    ///
    /// This performs actual I/O to query STUN servers and should be called
    /// in an async context.
    async fn gather_srflx_candidates(
        runtime: Arc<dyn Runtime>,
        sockets: Vec<Arc<dyn AsyncUdpSocket>>,
        ice_servers: Vec<RTCIceServer>,
        msg_tx: Sender<MessageInner>,
    ) -> Result<()> {
        for ice_server in ice_servers {
            for url in &ice_server.urls {
                // Only handle stun: URLs for now
                if !url.starts_with("stun:") {
                    continue;
                }

                for socket in &sockets {
                    let local_addr = socket.local_addr()?;

                    let candidate_init =
                        RTCIceGatherer::gather_from_stun_server(runtime.clone(), local_addr, url)
                            .await?;

                    msg_tx
                        .send(MessageInner::LocalIceCandidate(candidate_init))
                        .await
                        .map_err(|e| Error::Other(e.to_string()))?;
                }
            }
        }

        Ok(())
    }

    /// Gather a single srflx candidate from a STUN server
    async fn gather_from_stun_server(
        runtime: Arc<dyn Runtime>,
        local_addr: SocketAddr,
        stun_url: &str,
    ) -> Result<RTCIceCandidateInit> {
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
            .ok_or(Error::Other(
                "Failed to resolve STUN server hostname".to_string(),
            ))?;

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
                Ok::<XorMappedAddress, Error>(xor_addr)
            } else {
                Err(Error::Other("No STUN response event".to_string()))
            }
        })
        .await
        .map_err(|_| Error::Other("STUN request timeout".to_string()))??;

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
