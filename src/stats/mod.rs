use ice::agent::agent_stats::{CandidatePairStats, CandidateStats};
use stats_collector::StatsCollector;

use std::sync::Arc;
use tokio::sync::Mutex;

pub mod stats_collector;

pub enum StatsType {
    CSRC,
    CandidatePair,
    Certificate,
    Codec,
    DataChannel,
    InboundRTP,
    LocalCandidate,
    OutboundRTP,
    PeerConnection,
    Receiver,
    RemoteCandidate,
    RemoteInboundRTP,
    RemoteOutboundRTP,
    Sender,
    Stream,
    Track,
    Transport,
}

pub enum StatsReportType {
  StatsType(ICECandidatePairStats),
}

impl From<CandidatePairStats> for StatsReportType {
    fn from(stats: CandidatePairStats) -> Self {
       StatsReportType::StatsType(stats.into())
    }
}

impl From<CandidateStats> for StatsReportType {
    fn from(stats: CandidateStats) -> Self {
       StatsReportType::StatsType(stats.into())
    }
}

pub struct StatsReport {}

impl From<Arc<Mutex<StatsCollector>>> for StatsReport {
    fn from(_collector: Arc<Mutex<StatsCollector>>) -> Self {
        StatsReport {}
    }
}

pub struct ICECandidatePairStats {}

impl From<CandidatePairStats> for ICECandidatePairStats {
    fn from(_stats: CandidatePairStats) -> Self {
        ICECandidatePairStats {}
    }
}

impl From<CandidateStats> for ICECandidatePairStats {
    fn from(_stats: CandidateStats) -> Self {
        ICECandidatePairStats {}
    }
}
