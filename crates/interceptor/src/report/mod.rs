use rtp::packetizer::FnTimeGen;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use waitgroup::WaitGroup;

pub mod rr;
pub mod sr;

use rr::{ReceiverReport, ReceiverReportInternal};
use sr::{SenderReport, SenderReportInternal};

/// ReceiverBuilder can be used to configure ReceiverReport Interceptor.
#[derive(Default)]
pub struct ReportBuilder {
    interval: Option<Duration>,
    now: Option<FnTimeGen>,
}

impl ReportBuilder {
    /// with_interval sets send interval for the interceptor.
    pub fn with_interval(mut self, interval: Duration) -> ReportBuilder {
        self.interval = Some(interval);
        self
    }

    /// with_now_fn sets an alternative for the time.Now function.
    pub fn with_now_fn(mut self, now: FnTimeGen) -> ReportBuilder {
        self.now = Some(now);
        self
    }

    pub fn build_rr(mut self) -> ReceiverReport {
        let (close_tx, close_rx) = mpsc::channel(1);
        ReceiverReport {
            internal: Arc::new(ReceiverReportInternal {
                interval: if let Some(interval) = self.interval.take() {
                    interval
                } else {
                    Duration::from_secs(1)
                },
                now: self.now.take(),
                parent_rtcp_reader: Mutex::new(None),
                streams: Mutex::new(HashMap::new()),
                close_rx: Mutex::new(Some(close_rx)),
            }),

            wg: Mutex::new(Some(WaitGroup::new())),
            close_tx: Mutex::new(Some(close_tx)),
        }
    }

    pub fn build_sr(mut self) -> SenderReport {
        let (close_tx, close_rx) = mpsc::channel(1);
        SenderReport {
            internal: Arc::new(SenderReportInternal {
                interval: if let Some(interval) = self.interval.take() {
                    interval
                } else {
                    Duration::from_secs(1)
                },
                now: self.now.take(),
                streams: Mutex::new(HashMap::new()),
                close_rx: Mutex::new(Some(close_rx)),
            }),

            wg: Mutex::new(Some(WaitGroup::new())),
            close_tx: Mutex::new(Some(close_tx)),
        }
    }
}
