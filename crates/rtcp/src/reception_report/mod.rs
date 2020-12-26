pub(crate) const RECEPTION_REPORT_LENGTH: usize = 24;
pub(crate) const FRACTION_LOST_OFFSET: usize = 4;
pub(crate) const TOTAL_LOST_OFFSET: usize = 5;
pub(crate) const LAST_SEQ_OFFSET: usize = 8;
pub(crate) const JITTER_OFFSET: usize = 12;
pub(crate) const LAST_SR_OFFSET: usize = 16;
pub(crate) const DELAY_OFFSET: usize = 20;

mod reception_report_def;

pub use reception_report_def::ReceptionReport;
