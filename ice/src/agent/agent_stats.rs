use std::sync::atomic::Ordering;

use tokio::time::Instant;

use crate::agent::agent_internal::AgentInternal;
use crate::candidate::{CandidatePairState, CandidateType};
use crate::network_type::NetworkType;

/// Contains ICE candidate pair statistics.
pub struct CandidatePairStats {
    /// The timestamp associated with this struct.
    pub timestamp: Instant,

    /// The id of the local candidate.
    pub local_candidate_id: String,

    /// The id of the remote candidate.
    pub remote_candidate_id: String,

    /// The state of the checklist for the local and remote candidates in a pair.
    pub state: CandidatePairState,

    /// It is true when this valid pair that should be used for media,
    /// if it is the highest-priority one amongst those whose nominated flag is set.
    pub nominated: bool,

    /// The total number of packets sent on this candidate pair.
    pub packets_sent: u32,

    /// The total number of packets received on this candidate pair.
    pub packets_received: u32,

    /// The total number of payload bytes sent on this candidate pair not including headers or
    /// padding.
    pub bytes_sent: u64,

    /// The total number of payload bytes received on this candidate pair not including headers or
    /// padding.
    pub bytes_received: u64,

    /// The timestamp at which the last packet was sent on this particular candidate pair, excluding
    /// STUN packets.
    pub last_packet_sent_timestamp: Instant,

    /// The timestamp at which the last packet was received on this particular candidate pair,
    /// excluding STUN packets.
    pub last_packet_received_timestamp: Instant,

    /// The timestamp at which the first STUN request was sent on this particular candidate pair.
    pub first_request_timestamp: Instant,

    /// The timestamp at which the last STUN request was sent on this particular candidate pair.
    /// The average interval between two consecutive connectivity checks sent can be calculated with
    /// (last_request_timestamp - first_request_timestamp) / requests_sent.
    pub last_request_timestamp: Instant,

    /// Timestamp at which the last STUN response was received on this particular candidate pair.
    pub last_response_timestamp: Instant,

    /// The sum of all round trip time measurements in seconds since the beginning of the session,
    /// based on STUN connectivity check responses (responses_received), including those that reply
    /// to requests that are sent in order to verify consent. The average round trip time can be
    /// computed from total_round_trip_time by dividing it by responses_received.
    pub total_round_trip_time: f64,

    /// The latest round trip time measured in seconds, computed from both STUN connectivity checks,
    /// including those that are sent for consent verification.
    pub current_round_trip_time: f64,

    /// It is calculated by the underlying congestion control by combining the available bitrate for
    /// all the outgoing RTP streams using this candidate pair. The bitrate measurement does not
    /// count the size of the IP or other transport layers like TCP or UDP. It is similar to the
    /// TIAS defined in RFC 3890, i.e., it is measured in bits per second and the bitrate is
    /// calculated over a 1 second window.
    pub available_outgoing_bitrate: f64,

    /// It is calculated by the underlying congestion control by combining the available bitrate for
    /// all the incoming RTP streams using this candidate pair. The bitrate measurement does not
    /// count the size of the IP or other transport layers like TCP or UDP. It is similar to the
    /// TIAS defined in  RFC 3890, i.e., it is measured in bits per second and the bitrate is
    /// calculated over a 1 second window.
    pub available_incoming_bitrate: f64,

    /// The number of times the circuit breaker is triggered for this particular 5-tuple,
    /// ceasing transmission.
    pub circuit_breaker_trigger_count: u32,

    /// The total number of connectivity check requests received (including retransmissions).
    /// It is impossible for the receiver to tell whether the request was sent in order to check
    /// connectivity or check consent, so all connectivity checks requests are counted here.
    pub requests_received: u64,

    /// The total number of connectivity check requests sent (not including retransmissions).
    pub requests_sent: u64,

    /// The total number of connectivity check responses received.
    pub responses_received: u64,

    /// The total number of connectivity check responses sent. Since we cannot distinguish
    /// connectivity check requests and consent requests, all responses are counted.
    pub responses_sent: u64,

    /// The total number of connectivity check request retransmissions received.
    pub retransmissions_received: u64,

    /// The total number of connectivity check request retransmissions sent.
    pub retransmissions_sent: u64,

    /// The total number of consent requests sent.
    pub consent_requests_sent: u64,

    /// The timestamp at which the latest valid STUN binding response expired.
    pub consent_expired_timestamp: Instant,
}

impl Default for CandidatePairStats {
    fn default() -> Self {
        Self {
            timestamp: Instant::now(),
            local_candidate_id: String::new(),
            remote_candidate_id: String::new(),
            state: CandidatePairState::default(),
            nominated: false,
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            last_packet_sent_timestamp: Instant::now(),
            last_packet_received_timestamp: Instant::now(),
            first_request_timestamp: Instant::now(),
            last_request_timestamp: Instant::now(),
            last_response_timestamp: Instant::now(),
            total_round_trip_time: 0.0,
            current_round_trip_time: 0.0,
            available_outgoing_bitrate: 0.0,
            available_incoming_bitrate: 0.0,
            circuit_breaker_trigger_count: 0,
            requests_received: 0,
            requests_sent: 0,
            responses_received: 0,
            responses_sent: 0,
            retransmissions_received: 0,
            retransmissions_sent: 0,
            consent_requests_sent: 0,
            consent_expired_timestamp: Instant::now(),
        }
    }
}

/// Contains ICE candidate statistics related to the `ICETransport` objects.
#[derive(Debug, Clone)]
pub struct CandidateStats {
    // The timestamp associated with this struct.
    pub timestamp: Instant,

    /// The candidate id.
    pub id: String,

    /// The type of network interface used by the base of a local candidate (the address the ICE
    /// agent sends from). Only present for local candidates; it's not possible to know what type of
    /// network interface a remote candidate is using.
    ///
    /// Note: This stat only tells you about the network interface used by the first "hop"; it's
    /// possible that a connection will be bottlenecked by another type of network.  For example,
    /// when using Wi-Fi tethering, the networkType of the relevant candidate would be "wifi", even
    /// when the next hop is over a cellular connection.
    pub network_type: NetworkType,

    /// The IP address of the candidate, allowing for IPv4 addresses and IPv6 addresses, but fully
    /// qualified domain names (FQDNs) are not allowed.
    pub ip: String,

    /// The port number of the candidate.
    pub port: u16,

    /// The `Type` field of the ICECandidate.
    pub candidate_type: CandidateType,

    /// The `priority` field of the ICECandidate.
    pub priority: u32,

    /// The url of the TURN or STUN server indicated in the that translated this IP address.
    /// It is the url address surfaced in an PeerConnectionICEEvent.
    pub url: String,

    /// The protocol used by the endpoint to communicate with the TURN server. This is only present
    /// for local candidates. Valid values for the TURN url protocol is one of udp, tcp, or tls.
    pub relay_protocol: String,

    /// It is true if the candidate has been deleted/freed. For host candidates, this means that any
    /// network resources (typically a socket) associated with the candidate have been released. For
    /// TURN candidates, this means the TURN allocation is no longer active.
    ///
    /// Only defined for local candidates. For remote candidates, this property is not applicable.
    pub deleted: bool,
}

impl Default for CandidateStats {
    fn default() -> Self {
        Self {
            timestamp: Instant::now(),
            id: String::new(),
            network_type: NetworkType::default(),
            ip: String::new(),
            port: 0,
            candidate_type: CandidateType::default(),
            priority: 0,
            url: String::new(),
            relay_protocol: String::new(),
            deleted: false,
        }
    }
}

impl AgentInternal {
    /// Returns a list of candidate pair stats.
    pub(crate) async fn get_candidate_pairs_stats(&self) -> Vec<CandidatePairStats> {
        let checklist = self.agent_conn.checklist.lock().await;
        let mut res = Vec::with_capacity(checklist.len());
        for cp in &*checklist {
            let stat = CandidatePairStats {
                timestamp: Instant::now(),
                local_candidate_id: cp.local.id(),
                remote_candidate_id: cp.remote.id(),
                state: cp.state.load(Ordering::SeqCst).into(),
                nominated: cp.nominated.load(Ordering::SeqCst),
                ..CandidatePairStats::default()
            };
            res.push(stat);
        }
        res
    }

    /// Returns a list of local candidates stats.
    pub(crate) async fn get_local_candidates_stats(&self) -> Vec<CandidateStats> {
        let local_candidates = self.local_candidates.lock().await;
        let mut res = Vec::with_capacity(local_candidates.len());
        for (network_type, local_candidates) in &*local_candidates {
            for c in local_candidates {
                let stat = CandidateStats {
                    timestamp: Instant::now(),
                    id: c.id(),
                    network_type: *network_type,
                    ip: c.address(),
                    port: c.port(),
                    candidate_type: c.candidate_type(),
                    priority: c.priority(),
                    // URL string
                    relay_protocol: "udp".to_owned(),
                    // Deleted bool
                    ..CandidateStats::default()
                };
                res.push(stat);
            }
        }
        res
    }

    /// Returns a list of remote candidates stats.
    pub(crate) async fn get_remote_candidates_stats(&self) -> Vec<CandidateStats> {
        let remote_candidates = self.remote_candidates.lock().await;
        let mut res = Vec::with_capacity(remote_candidates.len());
        for (network_type, remote_candidates) in &*remote_candidates {
            for c in remote_candidates {
                let stat = CandidateStats {
                    timestamp: Instant::now(),
                    id: c.id(),
                    network_type: *network_type,
                    ip: c.address(),
                    port: c.port(),
                    candidate_type: c.candidate_type(),
                    priority: c.priority(),
                    // URL string
                    relay_protocol: "udp".to_owned(),
                    // Deleted bool
                    ..CandidateStats::default()
                };
                res.push(stat);
            }
        }
        res
    }
}
