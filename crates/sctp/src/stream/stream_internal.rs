use super::*;

#[derive(Default)]
pub struct StreamInternal {
    pub(crate) max_payload_size: u32,

    pub(crate) stream_identifier: u16,
    pub(crate) default_payload_type: PayloadProtocolIdentifier,
    pub(crate) reassembly_queue: ReassemblyQueue,
    pub(crate) sequence_number: u16,
    pub(crate) read_notifier: Notify,
    pub(crate) read_err: Option<Error>,
    pub(crate) write_err: Option<Error>,
    pub(crate) unordered: bool,
    pub(crate) reliability_type: ReliabilityType,
    pub(crate) reliability_value: u32,
    pub(crate) buffered_amount: u64,
    pub(crate) buffered_amount_low: u64,
    pub(crate) on_buffered_amount_low: Option<OnBufferedAmountLowFn>,
    pub(crate) name: String,
}
