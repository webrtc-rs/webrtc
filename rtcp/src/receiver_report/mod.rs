mod receiver_report_def;
#[cfg(test)]
mod receiver_report_test;

use crate::header;

pub(super) const SSRC_LENGTH: usize = 4;
pub(super) const RR_SSRC_OFFSET: usize = header::HEADER_LENGTH;
pub(super) const RR_REPORT_OFFSET: usize = RR_SSRC_OFFSET + SSRC_LENGTH;
