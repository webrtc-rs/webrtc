//! ICE Candidate Gathering
//!
//! This module implements automatic ICE candidate gathering including:
//! - Host candidates from local network interfaces
//! - Server Reflexive (srflx) candidates via STUN
//! - Relay candidates via TURN (TODO)

use rtc::peer_connection::transport::RTCIceCandidateInit;
use rtc::peer_connection::configuration::RTCIceServer;
use std::net::SocketAddr;

/// Gather host ICE candidates from a local socket address
///
/// This creates a host candidate for the given local address that was
/// bound by the peer connection.
pub(crate) fn gather_host_candidates(local_addr: SocketAddr) -> Vec<RTCIceCandidateInit> {
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
/// This sends STUN binding requests to configured STUN servers to discover
/// the public IP address and port as seen by the STUN server.
pub(crate) async fn gather_srflx_candidates(
    local_addr: SocketAddr,
    ice_servers: &[RTCIceServer],
) -> Vec<RTCIceCandidateInit> {
    let mut candidates = Vec::new();

    // Parse STUN servers from ice_servers
    for server in ice_servers {
        for url in &server.urls {
            // Parse STUN URLs (format: "stun:hostname:port" or "stun:hostname")
            if let Some(stun_url) = url.strip_prefix("stun:") {
                log::info!("Gathering srflx candidate via STUN server: {}", url);
                
                // Try to gather from this STUN server
                match gather_from_stun_server(stun_url, local_addr, url).await {
                    Ok(candidate) => {
                        candidates.push(candidate);
                        break; // Only need one srflx candidate per component
                    }
                    Err(e) => {
                        log::warn!("Failed to gather from STUN server {}: {}", url, e);
                    }
                }
            }
        }
        
        if !candidates.is_empty() {
            break; // Got a successful srflx candidate
        }
    }

    candidates
}

/// Gather a server reflexive candidate from a single STUN server
///
/// This follows the pattern from trickle-ice-srflx example
async fn gather_from_stun_server(
    stun_url: &str,
    local_addr: SocketAddr,
    original_url: &str,
) -> Result<RTCIceCandidateInit, Box<dyn std::error::Error + Send + Sync>> {
    use bytes::BytesMut;
    use rtc::sansio::Protocol;
    use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
    use rtc::stun::client::{ClientBuilder};
    use rtc::stun::message::*;
    use rtc::stun::xoraddr::XorMappedAddress;
    use tokio::net::UdpSocket;
    use tokio::time::{timeout, Duration};
    use std::time::Instant;

    // Resolve STUN server address (add default port 3478 if not specified)
    let stun_server_addr_str = if stun_url.contains(':') {
        stun_url.to_string()
    } else {
        format!("{}:3478", stun_url)
    };

    log::debug!("Resolving STUN server: {}", stun_server_addr_str);
    
    // Resolve hostname to IP address
    let stun_server_addr: SocketAddr = tokio::net::lookup_host(&stun_server_addr_str)
        .await?
        .next()
        .ok_or("Failed to resolve STUN server hostname")?;

    log::debug!("Resolved STUN server {} to {}", stun_server_addr_str, stun_server_addr);

    // Create a temporary UDP socket for STUN (match IP version of STUN server)
    let bind_addr = if stun_server_addr.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    let stun_socket = UdpSocket::bind(bind_addr).await?;
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
        stun_socket.send_to(&transmit.message, stun_server_addr).await?;
        log::debug!("Sent STUN binding request to {}", stun_server_addr);
    }

    // Wait for response with timeout
    let xor_addr = timeout(Duration::from_secs(5), async {
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
    }).await??;

    // Close the STUN client
    client.close()?;

    // Create server reflexive candidate
    use rtc::ice::candidate::candidate_server_reflexive::CandidateServerReflexiveConfig;
    use rtc::ice::candidate::CandidateConfig;
    
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

    let mut candidate_init = rtc::peer_connection::transport::RTCIceCandidate::from(&candidate).to_json()?;
    candidate_init.url = Some(original_url.to_string());
    
    log::info!("Generated srflx candidate: {}", candidate_init.candidate);
    Ok(candidate_init)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gather_host_candidates() {
        let local_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let candidates = gather_host_candidates(local_addr);

        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].candidate.contains("127.0.0.1"));
        assert!(candidates[0].candidate.contains("8080"));
        assert!(candidates[0].candidate.contains("typ host"));
    }

    #[test]
    fn test_gather_host_candidates_ipv6() {
        let local_addr: SocketAddr = "[::1]:9090".parse().unwrap();
        let candidates = gather_host_candidates(local_addr);

        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].candidate.contains("::1"));
        assert!(candidates[0].candidate.contains("9090"));
        assert!(candidates[0].candidate.contains("typ host"));
    }
}
