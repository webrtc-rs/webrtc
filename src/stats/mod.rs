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

pub struct StatsReport {}

impl From<Arc<Mutex<StatsCollector>>> for StatsReport {
    fn from(_collector: Arc<Mutex<StatsCollector>>) -> Self {
        StatsReport {}
    }
}

