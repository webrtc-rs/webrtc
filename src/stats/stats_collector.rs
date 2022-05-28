use std::collections::HashMap;

use super::StatsReportType;

#[derive(Debug, Default)]
pub struct StatsCollector {
    pub(crate) reports: HashMap<String, StatsReportType>,
}

impl StatsCollector {
    pub(crate) fn new() -> Self {
        StatsCollector {
            ..Default::default()
        }
    }

    pub(crate) fn merge(&mut self, stats: HashMap<String, StatsReportType>) {
        self.reports.extend(stats)
    }

    pub(crate) fn insert(&mut self, id: String, stats: StatsReportType) {
        self.reports.insert(id, stats);
    }
}
