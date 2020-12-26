mod receiver_report_def;
mod receiver_report_test;

pub use receiver_report_def::ReceiverReport;

use crate::header;

pub(super) const SSRC_LENGTH: usize = 4;
pub(super) const RR_SSRC_OFFSET: usize = header::HEADER_LENGTH;
pub(super) const RR_REPORT_OFFSET: usize = RR_SSRC_OFFSET + SSRC_LENGTH;
