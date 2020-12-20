mod sender_report_def;
#[cfg(test)]
mod sender_report_test;

pub use sender_report_def::SenderReport;

pub(crate) const SR_HEADER_LENGTH: usize = 24;
pub(crate) const SR_SSRC_OFFSET: usize = 0;
pub(crate) const SR_NTP_OFFSET: usize = SR_SSRC_OFFSET + crate::header::SSRC_LENGTH;
pub(crate) const NTP_TIME_LENGTH: usize = 8;
pub(crate) const SR_RTP_OFFSET: usize = SR_NTP_OFFSET + NTP_TIME_LENGTH;
pub(crate) const RTP_TIME_LENGTH: usize = 4;
pub(crate) const SR_PACKET_COUNT_OFFSET: usize = SR_RTP_OFFSET + RTP_TIME_LENGTH;
pub(crate) const SR_PACKET_COUNT_LENGTH: usize = 4;
pub(crate) const SR_OCTET_COUNT_OFFSET: usize = SR_PACKET_COUNT_OFFSET + SR_PACKET_COUNT_LENGTH;
pub(crate) const SR_OCTET_COUNT_LENGTH: usize = 4;
pub(crate) const SR_REPORT_OFFSET: usize = SR_OCTET_COUNT_OFFSET + SR_OCTET_COUNT_LENGTH;
