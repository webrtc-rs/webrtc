//! ICE Candidate Gathering
//!
//! This module implements automatic ICE candidate gathering including:
//! - Host candidates from local network interfaces
//! - Server Reflexive (srflx) candidates via STUN
//! - Relay candidates via TURN

use rtc::peer_connection::transport::RTCIceCandidateInit;
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
