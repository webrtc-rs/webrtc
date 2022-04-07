use crate::data_channel::data_channel_state::RTCDataChannelState;
use crate::data_channel::RTCDataChannel;
use crate::dtls_transport::dtls_fingerprint::RTCDtlsFingerprint;
use crate::peer_connection::certificate::RTCCertificate;
use crate::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;
use crate::rtp_transceiver::PayloadType;
use crate::sctp_transport::RTCSctpTransport;

use ice::agent::agent_stats::{CandidatePairStats, CandidateStats};
use ice::candidate::{CandidatePairState, CandidateType};
use ice::network_type::NetworkType;
use stats_collector::StatsCollector;

use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Instant;

pub mod stats_collector;

pub enum SourceStatsType {
    CandidatePair(CandidatePairStats),
    LocalCandidate(CandidateStats),
    RemoteCandidate(CandidateStats),
}

#[derive(Debug)]
pub enum StatsReportType {
    CandidatePair(ICECandidatePairStats),
    CertificateStats(CertificateStats),
    Codec(CodecStats),
    DataChannel(DataChannelStats),
    LocalCandidate(ICECandidateStats),
    PeerConnection(PeerConnectionStats),
    RemoteCandidate(ICECandidateStats),
    SCTPTransport(ICETransportStats),
    Transport(ICETransportStats),
}

impl From<SourceStatsType> for StatsReportType {
    fn from(stats: SourceStatsType) -> Self {
        match stats {
            SourceStatsType::CandidatePair(stats) => StatsReportType::CandidatePair(stats.into()),
            SourceStatsType::LocalCandidate(stats) => StatsReportType::LocalCandidate(stats.into()),
            SourceStatsType::RemoteCandidate(stats) => {
                StatsReportType::RemoteCandidate(stats.into())
            }
        }
    }
}

// TODO: should this be some form of String-indexed map?
pub struct StatsReport {
    reports: Vec<StatsReportType>,
}

impl From<Arc<Mutex<StatsCollector>>> for StatsReport {
    fn from(collector: Arc<Mutex<StatsCollector>>) -> Self {
        let lock = Arc::try_unwrap(collector).unwrap();
        let collector = lock.into_inner();

        StatsReport {
            reports: collector.reports,
        }
    }
}

// TODO: should timestamps be cast here, or during serialization?
// func statsTimestampFrom(t time.Time) StatsTimestamp {
// 	return StatsTimestamp(t.UnixNano() / int64(time.Millisecond))
// }

#[derive(Debug)]
pub struct ICECandidatePairStats {
    timestamp: Instant, // StatsTimestamp
    id: String,
    local_candidate_id: String,
    remote_candidate_id: String,
    state: CandidatePairState,
    nominated: bool,
    packets_sent: u32,
    packets_received: u32,
    bytes_sent: u64,
    bytes_received: u64,
    last_packet_sent_timestamp: Instant,    // statsTimestampFrom
    last_packet_received_timstamp: Instant, // statsTimestampFrom
    first_request_timestamp: Instant,       // statsTimestampFrom
    last_request_timestamp: Instant,        // statsTimestampFrom
    total_round_trip_time: f64,
    current_round_trip_time: f64,
    available_outgoing_bitrate: f64,
    available_incoming_bitrate: f64,
    circuit_breaker_trigger_count: u32,
    requests_received: u64,
    requests_sent: u64,
    responses_received: u64,
    responses_sent: u64,
    retransmissions_sent: u64,
    consent_requests_sent: u64,
    consent_expired_timestamp: Instant, // statsTimestampFrom
}

impl From<CandidatePairStats> for ICECandidatePairStats {
    fn from(stats: CandidatePairStats) -> Self {
        ICECandidatePairStats {
            timestamp: stats.timestamp,
            id: format!("{}-{}", stats.local_candidate_id, stats.remote_candidate_id),
            local_candidate_id: stats.local_candidate_id,
            remote_candidate_id: stats.remote_candidate_id,
            state: stats.state,
            nominated: stats.nominated,
            packets_sent: stats.packets_sent,
            packets_received: stats.packets_received,
            bytes_sent: stats.bytes_sent,
            bytes_received: stats.bytes_received,
            last_packet_sent_timestamp: stats.last_packet_sent_timestamp,
            last_packet_received_timstamp: stats.last_packet_received_timestamp,
            first_request_timestamp: stats.first_request_timestamp,
            last_request_timestamp: stats.last_request_timestamp,
            total_round_trip_time: stats.total_round_trip_time,
            current_round_trip_time: stats.current_round_trip_time,
            available_outgoing_bitrate: stats.available_outgoing_bitrate,
            available_incoming_bitrate: stats.available_incoming_bitrate,
            circuit_breaker_trigger_count: stats.circuit_breaker_trigger_count,
            requests_received: stats.requests_received,
            requests_sent: stats.requests_sent,
            responses_received: stats.responses_received,
            responses_sent: stats.responses_sent,
            retransmissions_sent: stats.retransmissions_sent,
            consent_requests_sent: stats.consent_requests_sent,
            consent_expired_timestamp: stats.consent_expired_timestamp,
        }
    }
}

#[derive(Debug)]
pub struct ICECandidateStats {
    timestamp: Instant,
    id: String,
    candidate_type: CandidateType,
    deleted: bool,
    ip: String,
    network_type: NetworkType,
    port: u16,
    priority: u32,
    relay_protocol: String,
    url: String,
}

impl From<CandidateStats> for ICECandidateStats {
    fn from(stats: CandidateStats) -> Self {
        ICECandidateStats {
            timestamp: stats.timestamp,
            id: stats.id,
            network_type: stats.network_type,
            ip: stats.ip,
            port: stats.port,
            candidate_type: stats.candidate_type,
            priority: stats.priority,
            url: stats.url,
            relay_protocol: stats.relay_protocol,
            deleted: stats.deleted,
        }
    }
}

#[derive(Debug)]
pub struct ICETransportStats {
    timestamp: Instant,
    id: String,
    // bytes_received: u64,
    // bytes_sent: u64,
}

impl ICETransportStats {
    pub(crate) fn new(id: String) -> Self {
        ICETransportStats {
            id,
            timestamp: Instant::now(),
        }
    }
}

#[derive(Debug)]
pub struct CertificateStats {
    timestamp: Instant,
    id: String,
    // base64_certificate: String,
    fingerprint: String,
    fingerprint_algorithm: String,
    // issuer_certificate_id: String,
}

impl CertificateStats {
    pub(crate) fn new(cert: &RTCCertificate, fingerprint: RTCDtlsFingerprint) -> Self {
        CertificateStats {
            timestamp: Instant::now(),
            id: cert.stats_id.clone(),
            // TODO: base64_certificate
            fingerprint: fingerprint.value,
            fingerprint_algorithm: fingerprint.algorithm,
            // TODO: issuer_certificate_id
        }
    }
}

#[derive(Debug)]
pub struct CodecStats {
    timestamp: Instant,
    id: String,
    payload_type: PayloadType,
    mime_type: String,
    clock_rate: u32,
    channels: u16,
    sdp_fmtp_line: String,
}

impl From<&RTCRtpCodecParameters> for CodecStats {
    fn from(codec: &RTCRtpCodecParameters) -> Self {
        CodecStats {
            timestamp: Instant::now(),
            id: codec.stats_id.clone(),
            payload_type: codec.payload_type,
            mime_type: codec.capability.mime_type.clone(),
            clock_rate: codec.capability.clock_rate,
            channels: codec.capability.channels,
            sdp_fmtp_line: codec.capability.sdp_fmtp_line.clone(),
        }
    }
}

#[derive(Debug)]
pub struct DataChannelStats {
    timestamp: Instant,
    id: String,
    data_channel_identifier: u16,
    bytes_received: usize,
    bytes_sent: usize,
    label: String,
    messages_received: usize,
    messages_sent: usize,
    protocol: String,
    state: RTCDataChannelState,
}

impl From<&RTCDataChannel> for DataChannelStats {
    fn from(data_channel: &RTCDataChannel) -> Self {
        let state = data_channel.ready_state();

        let mut bytes_received = 0;
        let mut bytes_sent = 0;
        let mut messages_received = 0;
        let mut messages_sent = 0;

        let lock = data_channel.data_channel.try_lock().unwrap();

        if let Some(internal) = &*lock {
            bytes_received = internal.bytes_received();
            bytes_sent = internal.bytes_sent();
            messages_received = internal.messages_received();
            messages_sent = internal.messages_sent();
        }

        DataChannelStats {
            state,
            timestamp: Instant::now(),
            id: data_channel.stats_id.clone(),
            data_channel_identifier: data_channel.id(), // TODO: "The value is initially null"
            label: data_channel.label.clone(),
            protocol: data_channel.protocol.clone(),
            bytes_received,
            bytes_sent,
            messages_received,
            messages_sent,
        }
    }
}

#[derive(Debug)]
pub struct PeerConnectionStats {
    timestamp: Instant,
    id: String,
    data_channels_accepted: u32,
    data_channels_closed: u32,
    data_channels_opened: u32,
    data_channels_requested: u32,
}

impl PeerConnectionStats {
    pub fn new(transport: &RTCSctpTransport, stats_id: String, data_channels_closed: u32) -> Self {
        PeerConnectionStats {
            timestamp: Instant::now(),
            id: stats_id,
            data_channels_accepted: transport.data_channels_accepted(),
            data_channels_opened: transport.data_channels_opened(),
            data_channels_requested: transport.data_channels_requested(),
            data_channels_closed,
        }
    }
}
