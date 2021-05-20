use crate::chunk::chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier};
use crate::error::Error;
use crate::queue::reassembly_queue::ReassemblyQueue;

use crate::association::AssociationState;
use bytes::Bytes;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub enum ReliabilityType {
    /// ReliabilityTypeReliable is used for reliable transmission
    Reliable = 0,
    /// ReliabilityTypeRexmit is used for partial reliability by retransmission count
    Rexmit = 1,
    /// ReliabilityTypeTimed is used for partial reliability by retransmission duration
    Timed = 2,
}

impl Default for ReliabilityType {
    fn default() -> Self {
        ReliabilityType::Reliable
    }
}

impl fmt::Display for ReliabilityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ReliabilityType::Reliable => "Reliable",
            ReliabilityType::Rexmit => "Rexmit",
            ReliabilityType::Timed => "Timed",
        };
        write!(f, "{}", s)
    }
}

pub type OnBufferedAmountLowFn = Box<dyn Fn()>;

/// Stream represents an SCTP stream
#[derive(Default)]
pub struct Stream {
    pub(crate) max_payload_size: u32,
    pub(crate) max_message_size: Arc<AtomicU32>, // clone from association
    pub(crate) state: Arc<AtomicU8>,             // clone from association

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
    //log                 :logging.LeveledLogger
    pub(crate) name: String,
}

impl fmt::Debug for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Stream")
            .field("max_payload_size", &self.max_payload_size)
            .field("max_message_size", &self.max_message_size)
            .field("state", &self.state)
            .field("stream_identifier", &self.stream_identifier)
            .field("default_payload_type", &self.default_payload_type)
            .field("reassembly_queue", &self.reassembly_queue)
            .field("sequence_number", &self.sequence_number)
            .field("read_err", &self.read_err)
            .field("write_err", &self.write_err)
            .field("unordered", &self.unordered)
            .field("reliability_type", &self.reliability_type)
            .field("reliability_value", &self.reliability_value)
            .field("buffered_amount", &self.buffered_amount)
            .field("buffered_amount_low", &self.buffered_amount_low)
            .field("name", &self.name)
            .finish()
    }
}

impl Stream {
    pub fn new(
        name: String,
        stream_identifier: u16,
        max_payload_size: u32,
        max_message_size: Arc<AtomicU32>,
        state: Arc<AtomicU8>,
    ) -> Self {
        Stream {
            max_payload_size,
            max_message_size,
            state,

            stream_identifier,
            default_payload_type: PayloadProtocolIdentifier::Unknown,
            reassembly_queue: ReassemblyQueue::new(stream_identifier),
            sequence_number: 0,
            read_notifier: Notify::new(),
            read_err: None,
            write_err: None,
            unordered: false,
            reliability_type: ReliabilityType::Reliable,
            reliability_value: 0,
            buffered_amount: 0,
            buffered_amount_low: 0,
            on_buffered_amount_low: None,
            name,
        }
    }

    /// stream_identifier returns the Stream identifier associated to the stream.
    pub fn stream_identifier(&self) -> u16 {
        self.stream_identifier
    }

    /// set_default_payload_type sets the default payload type used by write.
    pub fn set_default_payload_type(&mut self, default_payload_type: PayloadProtocolIdentifier) {
        self.default_payload_type = default_payload_type;
    }

    /// set_reliability_params sets reliability parameters for this stream.
    pub fn set_reliability_params(
        &mut self,
        unordered: bool,
        rel_type: ReliabilityType,
        rel_val: u32,
    ) {
        log::debug!(
            "[{}] reliability params: ordered={} type={} value={}",
            self.name,
            !unordered,
            rel_type,
            rel_val
        );
        self.unordered = unordered;
        self.reliability_type = rel_type;
        self.reliability_value = rel_val;
    }

    /// read reads a packet of len(p) bytes, dropping the Payload Protocol Identifier.
    /// Returns EOF when the stream is reset or an error if the stream is closed
    /// otherwise.
    pub async fn read(&mut self, p: &mut [u8]) -> Result<usize, Error> {
        let (n, _) = self.read_sctp(p).await?;
        Ok(n)
    }

    /// read_sctp reads a packet of len(p) bytes and returns the associated Payload
    /// Protocol Identifier.
    /// Returns EOF when the stream is reset or an error if the stream is closed
    /// otherwise.
    pub async fn read_sctp(
        &mut self,
        p: &mut [u8],
    ) -> Result<(usize, PayloadProtocolIdentifier), Error> {
        loop {
            let result = self.reassembly_queue.read(p);
            if result.is_ok() {
                return result;
            } else if let Err(err) = result {
                if err == Error::ErrShortBuffer {
                    return Err(err);
                }
            }

            if let Some(err) = self.read_err {
                return Err(err);
            }

            self.read_notifier.notified().await;
        }
    }

    pub(crate) fn handle_data(&mut self, pd: ChunkPayloadData) {
        if self.reassembly_queue.push(pd) {
            let readable = self.reassembly_queue.is_readable();
            log::debug!("[{}] reassembly_queue readable={}", self.name, readable);
            if readable {
                log::debug!("[{}] read_notifier.signal()", self.name);
                self.read_notifier.notify_one();
                log::debug!("[{}] read_notifier.signal() done", self.name);
            }
        }
    }

    pub(crate) fn handle_forward_tsn_for_ordered(&mut self, ssn: u16) {
        if self.unordered {
            return; // unordered chunks are handled by handleForwardUnordered method
        }

        // Remove all chunks older than or equal to the new TSN from
        // the reassembly_queue.
        self.reassembly_queue.forward_tsn_for_ordered(ssn);
        let readable = self.reassembly_queue.is_readable();

        // Notify the reader asynchronously if there's a data chunk to read.
        if readable {
            self.read_notifier.notify_one();
        }
    }

    pub(crate) fn handle_forward_tsn_for_unordered(&mut self, new_cumulative_tsn: u32) {
        if !self.unordered {
            return; // ordered chunks are handled by handleForwardTSNOrdered method
        }

        // Remove all chunks older than or equal to the new TSN from
        // the reassembly_queue.
        self.reassembly_queue
            .forward_tsn_for_unordered(new_cumulative_tsn);
        let readable = self.reassembly_queue.is_readable();

        // Notify the reader asynchronously if there's a data chunk to read.
        if readable {
            self.read_notifier.notify_one();
        }
    }

    /// write writes len(p) bytes from p with the default Payload Protocol Identifier
    pub async fn write(&mut self, p: &Bytes) -> Result<usize, Error> {
        self.write_sctp(p, self.default_payload_type).await
    }

    /// write_sctp writes len(p) bytes from p to the DTLS connection
    pub async fn write_sctp(
        &mut self,
        p: &Bytes,
        ppi: PayloadProtocolIdentifier,
    ) -> Result<usize, Error> {
        if p.len() > self.max_message_size.load(Ordering::SeqCst) as usize {
            return Err(Error::ErrOutboundPacketTooLarge);
        }

        let state: AssociationState = self.state.load(Ordering::SeqCst).into();
        match state {
            AssociationState::ShutdownSent
            | AssociationState::ShutdownAckSent
            | AssociationState::ShutdownPending
            | AssociationState::ShutdownReceived => {
                if self.write_err.is_none() {
                    self.write_err = Some(Error::ErrStreamClosed);
                }
            }
            _ => {}
        };

        if let Some(err) = self.write_err {
            return Err(err);
        }

        let _chunks = self.packetize(p, ppi);

        //TODO: return len(p), s.association.sendPayloadData(chunks)
        Ok(p.len())
    }

    fn packetize(&mut self, raw: &Bytes, ppi: PayloadProtocolIdentifier) -> Vec<ChunkPayloadData> {
        let mut i = 0;
        let mut remaining = raw.len();

        // From draft-ietf-rtcweb-data-protocol-09, section 6:
        //   All Data Channel Establishment Protocol messages MUST be sent using
        //   ordered delivery and reliable transmission.
        let unordered = ppi != PayloadProtocolIdentifier::Dcep && self.unordered;

        let mut chunks = vec![];

        let head_abandoned = Arc::new(AtomicBool::new(false));
        let head_all_inflight = Arc::new(AtomicBool::new(false));
        while remaining != 0 {
            let fragment_size = std::cmp::min(self.max_payload_size as usize, remaining); //self.association.max_payload_size

            // Copy the userdata since we'll have to store it until acked
            // and the caller may re-use the buffer in the mean time
            let user_data = raw.slice(i..i + fragment_size);

            let chunk = ChunkPayloadData {
                stream_identifier: self.stream_identifier,
                user_data,
                unordered,
                beginning_fragment: i == 0,
                ending_fragment: remaining - fragment_size == 0,
                immediate_sack: false,
                payload_type: ppi,
                stream_sequence_number: self.sequence_number,
                abandoned: head_abandoned.clone(), // all fragmented chunks use the same abandoned
                all_inflight: head_all_inflight.clone(), // all fragmented chunks use the same all_inflight
                ..Default::default()
            };

            chunks.push(chunk);

            remaining -= fragment_size;
            i += fragment_size;
        }

        // RFC 4960 Sec 6.6
        // Note: When transmitting ordered and unordered data, an endpoint does
        // not increment its Stream Sequence Number when transmitting a DATA
        // chunk with U flag set to 1.
        if !unordered {
            self.sequence_number += 1;
        }

        self.buffered_amount += raw.len() as u64;
        log::trace!("[{}] buffered_amount = {}", self.name, self.buffered_amount);

        chunks
    }

    /// Close closes the write-direction of the stream.
    /// Future calls to write are not permitted after calling Close.
    pub fn close(&mut self) -> Result<(), Error> {
        let (_sid, _is_open) = {
            let mut is_open = true;
            if self.write_err.is_none() {
                self.write_err = Some(Error::ErrStreamClosed);
            } else {
                is_open = false;
            }

            if self.read_err.is_none() {
                self.read_err = Some(Error::ErrEof);
            } else {
                is_open = false;
            }
            self.read_notifier.notify_waiters(); // broadcast regardless

            (self.stream_identifier, is_open)
        };

        //if is_open {
        // Reset the outgoing stream
        // https://tools.ietf.org/html/rfc6525
        //TODO: self.association.sendResetRequest(sid)

        //} else {
        Ok(())
        //}
    }

    /// buffered_amount returns the number of bytes of data currently queued to be sent over this stream.
    pub fn buffered_amount(&self) -> u64 {
        self.buffered_amount
    }

    /// buffered_amount_low_threshold returns the number of bytes of buffered outgoing data that is
    /// considered "low." Defaults to 0.
    pub fn buffered_amount_low_threshold(&self) -> u64 {
        self.buffered_amount_low
    }

    /// set_buffered_amount_low_threshold is used to update the threshold.
    /// See buffered_amount_low_threshold().
    pub fn set_buffered_amount_low_threshold(&mut self, th: u64) {
        self.buffered_amount_low = th;
    }

    /// on_buffered_amount_low sets the callback handler which would be called when the number of
    /// bytes of outgoing data buffered is lower than the threshold.
    pub fn on_buffered_amount_low(&mut self, f: OnBufferedAmountLowFn) {
        self.on_buffered_amount_low = Some(f);
    }

    /// This method is called by association's readLoop (go-)routine to notify this stream
    /// of the specified amount of outgoing data has been delivered to the peer.
    pub(crate) fn on_buffer_released(&mut self, n_bytes_released: i64) {
        if n_bytes_released <= 0 {
            return;
        }

        let from_amount = self.buffered_amount;

        if self.buffered_amount < n_bytes_released as u64 {
            self.buffered_amount = 0;
            log::error!(
                "[{}] released buffer size {} should be <= {}",
                self.name,
                n_bytes_released,
                self.buffered_amount
            )
        } else {
            self.buffered_amount -= n_bytes_released as u64;
        }

        log::trace!("[{}] buffered_amount = {}", self.name, self.buffered_amount);

        if let Some(f) = &self.on_buffered_amount_low {
            if from_amount > self.buffered_amount_low
                && self.buffered_amount <= self.buffered_amount_low
            {
                f();
                return;
            }
        }
    }

    pub(crate) fn get_num_bytes_in_reassembly_queue(&self) -> usize {
        // No lock is required as it reads the size with atomic load function.
        self.reassembly_queue.get_num_bytes()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_stream_buffered_amount() -> Result<(), Error> {
        let mut s = Stream::default();

        assert_eq!(0, s.buffered_amount());
        assert_eq!(0, s.buffered_amount_low_threshold());

        s.buffered_amount = 8192;
        s.set_buffered_amount_low_threshold(2048);
        assert_eq!(8192, s.buffered_amount(), "unexpected bufferedAmount");
        assert_eq!(
            2048,
            s.buffered_amount_low_threshold(),
            "unexpected threshold"
        );

        Ok(())
    }

    #[test]
    fn test_stream_amount_on_buffered_amount_low() -> Result<(), Error> {
        let mut s = Stream::default();

        s.buffered_amount = 4096;
        s.set_buffered_amount_low_threshold(2048);

        let n_cbs = Arc::new(AtomicU32::new(0));
        let n_cbs2 = n_cbs.clone();

        s.on_buffered_amount_low(Box::new(move || {
            n_cbs2.fetch_add(1, Ordering::SeqCst);
        }));

        // Negative value should be ignored (by design)
        s.on_buffer_released(-32); // bufferedAmount = 3072
        assert_eq!(4096, s.buffered_amount(), "unexpected bufferedAmount");
        assert_eq!(0, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

        // Above to above, no callback
        s.on_buffer_released(1024); // bufferedAmount = 3072
        assert_eq!(3072, s.buffered_amount(), "unexpected bufferedAmount");
        assert_eq!(0, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

        // Above to equal, callback should be made
        s.on_buffer_released(1024); // bufferedAmount = 2048
        assert_eq!(2048, s.buffered_amount(), "unexpected bufferedAmount");
        assert_eq!(1, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

        // Eaual to below, no callback
        s.on_buffer_released(1024); // bufferedAmount = 1024
        assert_eq!(1024, s.buffered_amount(), "unexpected bufferedAmount");
        assert_eq!(1, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

        // Blow to below, no callback
        s.on_buffer_released(1024); // bufferedAmount = 0
        assert_eq!(0, s.buffered_amount(), "unexpected bufferedAmount");
        assert_eq!(1, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

        // Capped at 0, no callback
        s.on_buffer_released(1024); // bufferedAmount = 0
        assert_eq!(0, s.buffered_amount(), "unexpected bufferedAmount");
        assert_eq!(1, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

        Ok(())
    }
}
