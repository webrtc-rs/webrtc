use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::sync::{mpsc, Mutex};
use waitgroup::WaitGroup;

pub mod receiver;
pub mod sender;

use receiver::{ReceiverReport, ReceiverReportInternal};
use sender::{SenderReport, SenderReportInternal};

use crate::error::Result;
use crate::{Interceptor, InterceptorBuilder};

type FnTimeGen = Arc<dyn Fn() -> SystemTime + Sync + 'static + Send>;

/// ReceiverBuilder can be used to configure ReceiverReport Interceptor.
#[derive(Default)]
pub struct ReportBuilder {
    is_rr: bool,
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

    fn build_rr(&self) -> ReceiverReport {
        let (close_tx, close_rx) = mpsc::channel(1);
        ReceiverReport {
            internal: Arc::new(ReceiverReportInternal {
                interval: if let Some(interval) = &self.interval {
                    *interval
                } else {
                    Duration::from_secs(1)
                },
                now: self.now.clone(),
                streams: Mutex::new(HashMap::new()),
                close_rx: Mutex::new(Some(close_rx)),
            }),

            wg: Mutex::new(Some(WaitGroup::new())),
            close_tx: Mutex::new(Some(close_tx)),
        }
    }

    fn build_sr(&self) -> SenderReport {
        let (close_tx, close_rx) = mpsc::channel(1);
        SenderReport {
            internal: Arc::new(SenderReportInternal {
                interval: if let Some(interval) = &self.interval {
                    *interval
                } else {
                    Duration::from_secs(1)
                },
                now: self.now.clone(),
                streams: Mutex::new(HashMap::new()),
                close_rx: Mutex::new(Some(close_rx)),
            }),

            wg: Mutex::new(Some(WaitGroup::new())),
            close_tx: Mutex::new(Some(close_tx)),
        }
    }
}

impl InterceptorBuilder for ReportBuilder {
    fn build(&self, _id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        if self.is_rr {
            Ok(Arc::new(self.build_rr()))
        } else {
            Ok(Arc::new(self.build_sr()))
        }
    }
}
