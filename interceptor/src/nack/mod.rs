use crate::stream_info::StreamInfo;

pub mod generator;
pub mod responder;

const UINT16SIZE_HALF: u16 = 1 << 15;

fn stream_support_nack(info: &StreamInfo) -> bool {
    for fb in &info.rtcp_feedback {
        if fb.typ == "nack" && fb.parameter.is_empty() {
            return true;
        }
    }

    false
}
