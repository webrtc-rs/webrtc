use super::StatsReportType;

#[derive(Debug, Default)]
pub struct StatsCollector {
    pub(crate) reports: Vec<StatsReportType>,
}

impl StatsCollector {
    pub(crate) fn new() -> Self {
        StatsCollector {
            ..Default::default()
        }
    }

    pub(crate) fn append(&mut self, stats: &mut Vec<StatsReportType>) {
        self.reports.append(stats);
    }

    pub(crate) fn push(&mut self, stats: StatsReportType) {
        self.reports.push(stats);
    }
}
