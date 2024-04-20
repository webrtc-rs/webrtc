use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use ice::agent::agent_stats::{CandidatePairStats, CandidateStats};
use ice::agent::Agent;
use ice::candidate::{CandidatePairState, CandidateType};
use ice::network_type::NetworkType;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;
use stats_collector::StatsCollector;
use tokio::time::Instant;

use crate::data_channel::data_channel_state::RTCDataChannelState;
use crate::data_channel::RTCDataChannel;
use crate::dtls_transport::dtls_fingerprint::RTCDtlsFingerprint;
use crate::peer_connection::certificate::RTCCertificate;
use crate::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;
use crate::rtp_transceiver::{PayloadType, SSRC};
use crate::sctp_transport::RTCSctpTransport;

mod serialize;
pub mod stats_collector;

#[derive(Debug, Serialize, Deserialize)]
pub enum RTCStatsType {
    #[serde(rename = "candidate-pair")]
    CandidatePair,
    #[serde(rename = "certificate")]
    Certificate,
    #[serde(rename = "codec")]
    Codec,
    #[serde(rename = "csrc")]
    CSRC,
    #[serde(rename = "data-channel")]
    DataChannel,
    #[serde(rename = "inbound-rtp")]
    InboundRTP,
    #[serde(rename = "local-candidate")]
    LocalCandidate,
    #[serde(rename = "outbound-rtp")]
    OutboundRTP,
    #[serde(rename = "peer-connection")]
    PeerConnection,
    #[serde(rename = "receiver")]
    Receiver,
    #[serde(rename = "remote-candidate")]
    RemoteCandidate,
    #[serde(rename = "remote-inbound-rtp")]
    RemoteInboundRTP,
    #[serde(rename = "remote-outbound-rtp")]
    RemoteOutboundRTP,
    #[serde(rename = "sender")]
    Sender,
    #[serde(rename = "transport")]
    Transport,
}

pub enum SourceStatsType {
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
    InboundRTP(InboundRTPStats),
    OutboundRTP(OutboundRTPStats),
    RemoteInboundRTP(RemoteInboundRTPStats),
    RemoteOutboundRTP(RemoteOutboundRTPStats),
}

impl From<SourceStatsType> for StatsReportType {
    fn from(stats: SourceStatsType) -> Self {
        match stats {
            SourceStatsType::LocalCandidate(stats) => StatsReportType::LocalCandidate(
                ICECandidateStats::new(stats, RTCStatsType::LocalCandidate),
            ),
            SourceStatsType::RemoteCandidate(stats) => StatsReportType::RemoteCandidate(
                ICECandidateStats::new(stats, RTCStatsType::RemoteCandidate),
            ),
        }
    }
}

impl Serialize for StatsReportType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            StatsReportType::CandidatePair(stats) => stats.serialize(serializer),
            StatsReportType::CertificateStats(stats) => stats.serialize(serializer),
            StatsReportType::Codec(stats) => stats.serialize(serializer),
            StatsReportType::DataChannel(stats) => stats.serialize(serializer),
            StatsReportType::LocalCandidate(stats) => stats.serialize(serializer),
            StatsReportType::PeerConnection(stats) => stats.serialize(serializer),
            StatsReportType::RemoteCandidate(stats) => stats.serialize(serializer),
            StatsReportType::SCTPTransport(stats) => stats.serialize(serializer),
            StatsReportType::Transport(stats) => stats.serialize(serializer),
            StatsReportType::InboundRTP(stats) => stats.serialize(serializer),
            StatsReportType::OutboundRTP(stats) => stats.serialize(serializer),
            StatsReportType::RemoteInboundRTP(stats) => stats.serialize(serializer),
            StatsReportType::RemoteOutboundRTP(stats) => stats.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for StatsReportType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let type_field = value
            .get("type")
            .ok_or_else(|| serde::de::Error::missing_field("type"))?;
        let rtc_type: RTCStatsType = serde_json::from_value(type_field.clone()).map_err(|e| {
            serde::de::Error::custom(format!(
                "failed to deserialize RTCStatsType from the `type` field ({}): {}",
                type_field, e
            ))
        })?;

        match rtc_type {
            RTCStatsType::CandidatePair => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::CandidatePair(stats))
            }
            RTCStatsType::Certificate => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::CertificateStats(stats))
            }
            RTCStatsType::Codec => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::Codec(stats))
            }
            RTCStatsType::CSRC => {
                todo!()
            }
            RTCStatsType::DataChannel => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::DataChannel(stats))
            }
            RTCStatsType::InboundRTP => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::InboundRTP(stats))
            }
            RTCStatsType::LocalCandidate => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::LocalCandidate(stats))
            }
            RTCStatsType::OutboundRTP => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::OutboundRTP(stats))
            }
            RTCStatsType::PeerConnection => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::PeerConnection(stats))
            }
            RTCStatsType::Receiver => {
                todo!()
            }
            RTCStatsType::RemoteCandidate => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::RemoteCandidate(stats))
            }
            RTCStatsType::RemoteInboundRTP => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::RemoteInboundRTP(stats))
            }
            RTCStatsType::RemoteOutboundRTP => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::RemoteOutboundRTP(stats))
            }
            RTCStatsType::Sender => {
                todo!()
            }
            RTCStatsType::Transport => {
                let stats = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(StatsReportType::Transport(stats))
            }
        }
    }
}

#[derive(Debug)]
pub struct StatsReport {
    pub reports: HashMap<String, StatsReportType>,
}

impl From<StatsCollector> for StatsReport {
    fn from(collector: StatsCollector) -> Self {
        StatsReport {
            reports: collector.into_reports(),
        }
    }
}

impl Serialize for StatsReport {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.reports.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StatsReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let root = value
            .as_object()
            .ok_or(serde::de::Error::custom("root object missing"))?;

        let mut reports = HashMap::new();
        for (key, value) in root {
            let report = serde_json::from_value(value.clone()).map_err(|e| {
                serde::de::Error::custom(format!(
                    "failed to deserialize `StatsReportType` from key={}, value={}: {}",
                    key, value, e
                ))
            })?;
            reports.insert(key.clone(), report);
        }
        Ok(Self { reports })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ICECandidatePairStats {
    // RTCStats
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub timestamp: Instant,
    #[serde(rename = "type")]
    pub stats_type: RTCStatsType,
    pub id: String,

    // RTCIceCandidatePairStats
    // TODO: Add `transportId`
    pub local_candidate_id: String,
    pub remote_candidate_id: String,
    pub state: CandidatePairState,
    pub nominated: bool,
    pub packets_sent: u32,
    pub packets_received: u32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub last_packet_sent_timestamp: Instant,
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub last_packet_received_timestamp: Instant,
    pub total_round_trip_time: f64,
    pub current_round_trip_time: f64,
    pub available_outgoing_bitrate: f64,
    pub available_incoming_bitrate: f64,
    pub requests_received: u64,
    pub requests_sent: u64,
    pub responses_received: u64,
    pub responses_sent: u64,
    pub consent_requests_sent: u64,
    // TODO: Add `packetsDiscardedOnSend`
    // TODO: Add `bytesDiscardedOnSend`

    // Non-canon
    pub circuit_breaker_trigger_count: u32,
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub consent_expired_timestamp: Instant,
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub first_request_timestamp: Instant,
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub last_request_timestamp: Instant,
    pub retransmissions_sent: u64,
}

impl From<CandidatePairStats> for ICECandidatePairStats {
    fn from(stats: CandidatePairStats) -> Self {
        ICECandidatePairStats {
            available_incoming_bitrate: stats.available_incoming_bitrate,
            available_outgoing_bitrate: stats.available_outgoing_bitrate,
            bytes_received: stats.bytes_received,
            bytes_sent: stats.bytes_sent,
            circuit_breaker_trigger_count: stats.circuit_breaker_trigger_count,
            consent_expired_timestamp: stats.consent_expired_timestamp,
            consent_requests_sent: stats.consent_requests_sent,
            current_round_trip_time: stats.current_round_trip_time,
            first_request_timestamp: stats.first_request_timestamp,
            id: format!("{}-{}", stats.local_candidate_id, stats.remote_candidate_id),
            last_packet_received_timestamp: stats.last_packet_received_timestamp,
            last_packet_sent_timestamp: stats.last_packet_sent_timestamp,
            last_request_timestamp: stats.last_request_timestamp,
            local_candidate_id: stats.local_candidate_id,
            nominated: stats.nominated,
            packets_received: stats.packets_received,
            packets_sent: stats.packets_sent,
            remote_candidate_id: stats.remote_candidate_id,
            requests_received: stats.requests_received,
            requests_sent: stats.requests_sent,
            responses_received: stats.responses_received,
            responses_sent: stats.responses_sent,
            retransmissions_sent: stats.retransmissions_sent,
            state: stats.state,
            stats_type: RTCStatsType::CandidatePair,
            timestamp: stats.timestamp,
            total_round_trip_time: stats.total_round_trip_time,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ICECandidateStats {
    // RTCStats
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub timestamp: Instant,
    #[serde(rename = "type")]
    pub stats_type: RTCStatsType,
    pub id: String,

    // RTCIceCandidateStats
    pub candidate_type: CandidateType,
    pub deleted: bool,
    pub ip: String,
    pub network_type: NetworkType,
    pub port: u16,
    pub priority: u32,
    pub relay_protocol: String,
    pub url: String,
}

impl ICECandidateStats {
    fn new(stats: CandidateStats, stats_type: RTCStatsType) -> Self {
        ICECandidateStats {
            candidate_type: stats.candidate_type,
            deleted: stats.deleted,
            id: stats.id,
            ip: stats.ip,
            network_type: stats.network_type,
            port: stats.port,
            priority: stats.priority,
            relay_protocol: stats.relay_protocol,
            stats_type,
            timestamp: stats.timestamp,
            url: stats.url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ICETransportStats {
    // RTCStats
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub timestamp: Instant,
    #[serde(rename = "type")]
    pub stats_type: RTCStatsType,
    pub id: String,

    // Non-canon
    pub bytes_received: usize,
    pub bytes_sent: usize,
}

impl ICETransportStats {
    pub(crate) fn new(id: String, agent: Arc<Agent>) -> Self {
        ICETransportStats {
            id,
            bytes_received: agent.get_bytes_received(),
            bytes_sent: agent.get_bytes_sent(),
            stats_type: RTCStatsType::Transport,
            timestamp: Instant::now(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateStats {
    // RTCStats
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub timestamp: Instant,
    #[serde(rename = "type")]
    pub stats_type: RTCStatsType,
    pub id: String,

    // RTCCertificateStats
    pub fingerprint: String,
    pub fingerprint_algorithm: String,
    // TODO: Add `base64Certificate` and `issuerCertificateId`.
}

impl CertificateStats {
    pub(crate) fn new(cert: &RTCCertificate, fingerprint: RTCDtlsFingerprint) -> Self {
        CertificateStats {
            // TODO: base64_certificate
            fingerprint: fingerprint.value,
            fingerprint_algorithm: fingerprint.algorithm,
            id: cert.stats_id.clone(),
            // TODO: issuer_certificate_id
            stats_type: RTCStatsType::Certificate,
            timestamp: Instant::now(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodecStats {
    // RTCStats
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub timestamp: Instant,
    #[serde(rename = "type")]
    pub stats_type: RTCStatsType,
    pub id: String,

    // RTCCodecStats
    pub payload_type: PayloadType,
    pub mime_type: String,
    pub channels: u16,
    pub clock_rate: u32,
    pub sdp_fmtp_line: String,
    // TODO: Add `transportId`
}

impl From<&RTCRtpCodecParameters> for CodecStats {
    fn from(codec: &RTCRtpCodecParameters) -> Self {
        CodecStats {
            channels: codec.capability.channels,
            clock_rate: codec.capability.clock_rate,
            id: codec.stats_id.clone(),
            mime_type: codec.capability.mime_type.clone(),
            payload_type: codec.payload_type,
            sdp_fmtp_line: codec.capability.sdp_fmtp_line.clone(),
            stats_type: RTCStatsType::Codec,
            timestamp: Instant::now(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataChannelStats {
    // RTCStats
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub timestamp: Instant,
    #[serde(rename = "type")]
    pub stats_type: RTCStatsType,
    pub id: String,

    // RTCDataChannelStats
    pub bytes_received: usize,
    pub bytes_sent: usize,
    pub data_channel_identifier: u16,
    pub label: String,
    pub messages_received: usize,
    pub messages_sent: usize,
    pub protocol: String,
    pub state: RTCDataChannelState,
}

impl DataChannelStats {
    pub(crate) async fn from(data_channel: &RTCDataChannel) -> Self {
        let state = data_channel.ready_state();

        let mut bytes_received = 0;
        let mut bytes_sent = 0;
        let mut messages_received = 0;
        let mut messages_sent = 0;

        let lock = data_channel.data_channel.lock().await;

        if let Some(internal) = &*lock {
            bytes_received = internal.bytes_received();
            bytes_sent = internal.bytes_sent();
            messages_received = internal.messages_received();
            messages_sent = internal.messages_sent();
        }

        Self {
            bytes_received,
            bytes_sent,
            data_channel_identifier: data_channel.id(), // TODO: "The value is initially null"
            id: data_channel.stats_id.clone(),
            label: data_channel.label.clone(),
            messages_received,
            messages_sent,
            protocol: data_channel.protocol.clone(),
            state,
            stats_type: RTCStatsType::DataChannel,
            timestamp: Instant::now(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerConnectionStats {
    // RTCStats
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub timestamp: Instant,
    #[serde(rename = "type")]
    pub stats_type: RTCStatsType,
    pub id: String,

    // RTCPeerConnectionStats
    pub data_channels_closed: u32,
    pub data_channels_opened: u32,

    // Non-canon
    pub data_channels_accepted: u32,
    pub data_channels_requested: u32,
}

impl PeerConnectionStats {
    pub fn new(transport: &RTCSctpTransport, stats_id: String, data_channels_closed: u32) -> Self {
        PeerConnectionStats {
            data_channels_accepted: transport.data_channels_accepted(),
            data_channels_closed,
            data_channels_opened: transport.data_channels_opened(),
            data_channels_requested: transport.data_channels_requested(),
            id: stats_id,
            stats_type: RTCStatsType::PeerConnection,
            timestamp: Instant::now(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundRTPStats {
    // RTCStats
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub timestamp: Instant,
    #[serde(rename = "type")]
    pub stats_type: RTCStatsType,
    pub id: String,

    // RTCRtpStreamStats
    pub ssrc: SSRC,
    pub kind: String, // Either "video" or "audio"
    // TODO: Add transportId
    // TODO: Add codecId

    // RTCReceivedRtpStreamStats
    pub packets_received: u64,
    // TODO: packetsLost
    // TODO: jitter(maybe, might be uattainable for the same reason as `framesDropped`)
    // NB: `framesDropped` can't be produced since we aren't decoding, might be worth introducing a
    // way for consumers to control this in the future.

    // RTCInboundRtpStreamStats
    pub track_identifier: String,
    pub mid: SmolStr,
    // TODO: `remoteId`
    // NB: `framesDecoded`, `frameWidth`, frameHeight`, `framesPerSecond`, `qpSum`,
    // `totalDecodeTime`, `totalInterFrameDelay`, and `totalSquaredInterFrameDelay` are all decoder
    // specific values and can't be produced since we aren't decoding.
    pub last_packet_received_timestamp: Option<SystemTime>,
    pub header_bytes_received: u64,
    // TODO: `packetsDiscarded`. This value only makes sense if we have jitter buffer, which we
    // cannot assume.
    // TODO: `fecPacketsReceived`, `fecPacketsDiscarded`
    pub bytes_received: u64,
    pub nack_count: u64,
    pub fir_count: Option<u64>,
    pub pli_count: Option<u64>,
    // NB: `totalProcessingDelay`, `estimatedPlayoutTimestamp`, `jitterBufferDelay`,
    // `jitterBufferTargetDelay`, `jitterBufferEmittedCount`, `jitterBufferMinimumDelay`,
    // `totalSamplesReceived`, `concealedSamples`, `silentConcealedSamples`, `concealmentEvents`,
    // `insertedSamplesForDeceleration`, `removedSamplesForAcceleration`, `audioLevel`,
    // `totalAudioEneregy`, `totalSampleDuration`, `framesReceived, and `decoderImplementation` are
    // all decoder specific and can't be produced since we aren't decoding.
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundRTPStats {
    // RTCStats
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub timestamp: Instant,
    #[serde(rename = "type")]
    pub stats_type: RTCStatsType,
    pub id: String,

    // RTCRtpStreamStats
    pub ssrc: SSRC,
    pub kind: String, // Either "video" or "audio"
    // TODO: Add transportId
    // TODO: Add codecId

    // RTCSentRtpStreamStats
    pub packets_sent: u64,
    pub bytes_sent: u64,

    // RTCOutboundRtpStreamStats
    // NB: non-canon in browsers this is available via `RTCMediaSourceStats` which we are unlikely to implement
    pub track_identifier: String,
    pub mid: SmolStr,
    // TODO: `mediaSourceId` and `remoteId`
    pub rid: Option<SmolStr>,
    pub header_bytes_sent: u64,
    // TODO: `retransmittedPacketsSent` and `retransmittedPacketsSent`
    // NB: `targetBitrate`, `totalEncodedBytesTarget`, `frameWidth` `frameHeight`, `framesPerSecond`, `framesSent`,
    // `hugeFramesSent`, `framesEncoded`, `keyFramesEncoded`, `qpSum`, and `totalEncodeTime` are
    // all encoder specific and can't be produced snce we aren't encoding.
    // TODO: `totalPacketSendDelay` time from `TrackLocalWriter::write_rtp` to being written to
    // socket.

    // NB: `qualityLimitationReason`, `qualityLimitationDurations`, and `qualityLimitationResolutionChanges` are all
    // encoder specific and can't be produced since we aren't encoding.
    pub nack_count: u64,
    pub fir_count: Option<u64>,
    pub pli_count: Option<u64>,
    // NB: `encoderImplementation` is encoder specific and can't be produced since we aren't
    // encoding.
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInboundRTPStats {
    // RTCStats
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub timestamp: Instant,
    #[serde(rename = "type")]
    pub stats_type: RTCStatsType,
    pub id: String,

    // RTCRtpStreamStats
    pub ssrc: SSRC,
    pub kind: String, // Either "video" or "audio"
    // TODO: Add transportId
    // TODO: Add codecId

    // RTCReceivedRtpStreamStats
    pub packets_received: u64,
    pub packets_lost: i64,
    // TODO: jitter(maybe, might be uattainable for the same reason as `framesDropped`)
    // NB: `framesDropped` can't be produced since we aren't decoding, might be worth introducing a
    // way for consumers to control this in the future.

    // RTCRemoteInboundRtpStreamStats
    pub local_id: String,
    pub round_trip_time: Option<f64>,
    pub total_round_trip_time: f64,
    pub fraction_lost: f64,
    pub round_trip_time_measurements: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteOutboundRTPStats {
    // RTCStats
    #[serde(with = "serialize::instant_to_epoch_seconds")]
    pub timestamp: Instant,
    #[serde(rename = "type")]
    pub stats_type: RTCStatsType,
    pub id: String,

    // RTCRtpStreamStats
    pub ssrc: SSRC,
    pub kind: String, // Either "video" or "audio"
    // TODO: Add transportId
    // TODO: Add codecId

    // RTCSentRtpStreamStats
    pub packets_sent: u64,
    pub bytes_sent: u64,

    // RTCRemoteOutboundRtpStreamStats
    pub local_id: String,
    // TODO: `remote_timestamp`
    pub round_trip_time: Option<f64>,
    pub reports_sent: u64,
    pub total_round_trip_time: f64,
    pub round_trip_time_measurements: u64,
}
