use super::context::srtcp::*;
use super::context::*;
use transport::replay_detector::*;

pub(crate) type ContextOption = Box<dyn Fn() -> Box<dyn ReplayDetector>>;

// srtp_replay_protection sets SRTP replay protection window size.
fn srtp_replay_protection(window_size: usize) -> ContextOption {
    Box::new(move || -> Box<dyn ReplayDetector> {
        Box::new(WrappedSlidingWindowDetector::new(
            window_size,
            MAX_SEQUENCE_NUMBER as u64,
        ))
    })
}

// SRTCPReplayProtection sets SRTCP replay protection window size.
fn srtcp_replay_protection(window_size: usize) -> ContextOption {
    Box::new(move || -> Box<dyn ReplayDetector> {
        Box::new(WrappedSlidingWindowDetector::new(
            window_size,
            MAX_SRTCP_INDEX,
        ))
    })
}

// srtp_no_replay_protection disables SRTP replay protection.
fn srtp_no_replay_protection() -> ContextOption {
    Box::new(|| -> Box<dyn ReplayDetector> { Box::new(NoOpReplayDetector::new()) })
}

// srtcp_no_replay_protection disables SRTCP replay protection.
fn srtcp_no_replay_protection() -> ContextOption {
    Box::new(|| -> Box<dyn ReplayDetector> { Box::new(NoOpReplayDetector::new()) })
}
