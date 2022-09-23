use crate::association::state::AssociationState;
use crate::association::Association;
use crate::chunk::chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier};
use crate::error::{Error, Result};
use crate::queue::reassembly_queue::{Chunks, ReassemblyQueue};
use crate::{ErrorCauseCode, Side};

use crate::util::{ByteSlice, BytesArray, BytesSource};
use bytes::Bytes;
use log::{debug, error, trace};
use std::fmt;

/// Identifier for a stream within a particular association
pub type StreamId = u16;

/// Application events about streams
#[derive(Debug, PartialEq, Eq)]
pub enum StreamEvent {
    /// One or more new streams has been opened
    Opened,
    /// A currently open stream has data or errors waiting to be read
    Readable {
        /// Which stream is now readable
        id: StreamId,
    },
    /// A formerly write-blocked stream might be ready for a write or have been stopped
    ///
    /// Only generated for streams that are currently open.
    Writable {
        /// Which stream is now writable
        id: StreamId,
    },
    /// A finished stream has been fully acknowledged or stopped
    Finished {
        /// Which stream has been finished
        id: StreamId,
    },
    /// The peer asked us to stop sending on an outgoing stream
    Stopped {
        /// Which stream has been stopped
        id: StreamId,
        /// Error code supplied by the peer
        error_code: ErrorCauseCode,
    },
    /// At least one new stream of a certain directionality may be opened
    Available,
    /// The number of bytes of outgoing data buffered is lower than the threshold.
    BufferedAmountLow {
        /// Which stream is now readable
        id: StreamId,
    },
}

/// Reliability type for stream
#[derive(Debug, Copy, Clone, PartialEq)]
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

impl From<u8> for ReliabilityType {
    fn from(v: u8) -> ReliabilityType {
        match v {
            1 => ReliabilityType::Rexmit,
            2 => ReliabilityType::Timed,
            _ => ReliabilityType::Reliable,
        }
    }
}

/// Stream represents an SCTP stream
pub struct Stream<'a> {
    pub(crate) stream_identifier: StreamId,
    pub(crate) association: &'a mut Association,
}

impl<'a> Stream<'a> {
    /// read reads a packet of len(p) bytes, dropping the Payload Protocol Identifier.
    /// Returns EOF when the stream is reset or an error if the stream is closed
    /// otherwise.
    pub fn read(&mut self) -> Result<Option<Chunks>> {
        self.read_sctp()
    }

    /// read_sctp reads a packet of len(p) bytes and returns the associated Payload
    /// Protocol Identifier.
    /// Returns EOF when the stream is reset or an error if the stream is closed
    /// otherwise.
    pub fn read_sctp(&mut self) -> Result<Option<Chunks>> {
        if let Some(s) = self.association.streams.get_mut(&self.stream_identifier) {
            if s.state == RecvSendState::ReadWritable || s.state == RecvSendState::Readable {
                return Ok(s.reassembly_queue.read());
            }
        }

        Err(Error::ErrStreamClosed)
    }

    /// write_sctp writes len(p) bytes from p to the DTLS connection
    pub fn write_sctp(&mut self, p: &Bytes, ppi: PayloadProtocolIdentifier) -> Result<usize> {
        self.write_source(&mut ByteSlice::from_slice(p), ppi)
    }

    /// Send data on the given stream
    ///
    /// Returns the number of bytes successfully written.
    pub fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.write_source(
            &mut ByteSlice::from_slice(data),
            self.get_default_payload_type()?,
        )
    }

    /// write writes len(p) bytes from p with the default Payload Protocol Identifier
    pub fn write_chunk(&mut self, p: &Bytes) -> Result<usize> {
        self.write_source(
            &mut ByteSlice::from_slice(p),
            self.get_default_payload_type()?,
        )
    }

    /// Send data on the given stream
    ///
    /// Returns the number of bytes and chunks successfully written.
    /// Note that this method might also write a partial chunk. In this case
    /// it will not count this chunk as fully written. However
    /// the chunk will be advanced and contain only non-written data after the call.
    pub fn write_chunks(&mut self, data: &mut [Bytes]) -> Result<usize> {
        self.write_source(
            &mut BytesArray::from_chunks(data),
            self.get_default_payload_type()?,
        )
    }

    /// write_source writes BytesSource to the DTLS connection
    fn write_source<B: BytesSource>(
        &mut self,
        source: &mut B,
        ppi: PayloadProtocolIdentifier,
    ) -> Result<usize> {
        if !self.is_writable() {
            return Err(Error::ErrStreamClosed);
        }

        if source.remaining() > self.association.max_message_size() as usize {
            return Err(Error::ErrOutboundPacketTooLarge);
        }

        let state: AssociationState = self.association.state();
        match state {
            AssociationState::ShutdownSent
            | AssociationState::ShutdownAckSent
            | AssociationState::ShutdownPending
            | AssociationState::ShutdownReceived => return Err(Error::ErrStreamClosed),
            _ => {}
        };

        let (p, _) = source.pop_chunk(self.association.max_message_size() as usize);

        if let Some(s) = self.association.streams.get_mut(&self.stream_identifier) {
            let chunks = s.packetize(&p, ppi);
            self.association.send_payload_data(chunks)?;

            Ok(p.len())
        } else {
            Err(Error::ErrStreamClosed)
        }
    }

    pub fn is_readable(&self) -> bool {
        if let Some(s) = self.association.streams.get(&self.stream_identifier) {
            s.state == RecvSendState::Readable || s.state == RecvSendState::ReadWritable
        } else {
            false
        }
    }

    pub fn is_writable(&self) -> bool {
        if let Some(s) = self.association.streams.get(&self.stream_identifier) {
            s.state == RecvSendState::Writable || s.state == RecvSendState::ReadWritable
        } else {
            false
        }
    }

    /// stop closes the read-direction of the stream.
    /// Future calls to read are not permitted after calling stop.
    pub fn stop(&mut self) -> Result<()> {
        let mut reset = false;
        if let Some(s) = self.association.streams.get_mut(&self.stream_identifier) {
            if s.state == RecvSendState::Readable || s.state == RecvSendState::ReadWritable {
                reset = true;
            }
            s.state = ((s.state as u8) & 0x2).into();
        }

        if reset {
            // Reset the outgoing stream
            // https://tools.ietf.org/html/rfc6525
            self.association
                .send_reset_request(self.stream_identifier)?;
        }

        Ok(())
    }

    /// finish closes the write-direction of the stream.
    /// Future calls to write are not permitted after calling Close.
    pub fn finish(&mut self) -> Result<()> {
        if let Some(s) = self.association.streams.get_mut(&self.stream_identifier) {
            s.state = ((s.state as u8) & 0x1).into();
        }
        Ok(())
    }

    /// stream_identifier returns the Stream identifier associated to the stream.
    pub fn stream_identifier(&self) -> StreamId {
        self.stream_identifier
    }

    /// set_default_payload_type sets the default payload type used by write.
    pub fn set_default_payload_type(
        &mut self,
        default_payload_type: PayloadProtocolIdentifier,
    ) -> Result<()> {
        if let Some(s) = self.association.streams.get_mut(&self.stream_identifier) {
            s.default_payload_type = default_payload_type;
            Ok(())
        } else {
            Err(Error::ErrStreamClosed)
        }
    }

    /// get_default_payload_type returns the payload type associated to the stream.
    pub fn get_default_payload_type(&self) -> Result<PayloadProtocolIdentifier> {
        if let Some(s) = self.association.streams.get(&self.stream_identifier) {
            Ok(s.default_payload_type)
        } else {
            Err(Error::ErrStreamClosed)
        }
    }

    /// set_reliability_params sets reliability parameters for this stream.
    pub fn set_reliability_params(
        &mut self,
        unordered: bool,
        rel_type: ReliabilityType,
        rel_val: u32,
    ) -> Result<()> {
        if let Some(s) = self.association.streams.get_mut(&self.stream_identifier) {
            debug!(
                "[{}] reliability params: ordered={} type={} value={}",
                s.side, !unordered, rel_type, rel_val
            );
            s.unordered = unordered;
            s.reliability_type = rel_type;
            s.reliability_value = rel_val;
            Ok(())
        } else {
            Err(Error::ErrStreamClosed)
        }
    }

    /// buffered_amount returns the number of bytes of data currently queued to be sent over this stream.
    pub fn buffered_amount(&self) -> Result<usize> {
        if let Some(s) = self.association.streams.get(&self.stream_identifier) {
            Ok(s.buffered_amount)
        } else {
            Err(Error::ErrStreamClosed)
        }
    }

    /// buffered_amount_low_threshold returns the number of bytes of buffered outgoing data that is
    /// considered "low." Defaults to 0.
    pub fn buffered_amount_low_threshold(&self) -> Result<usize> {
        if let Some(s) = self.association.streams.get(&self.stream_identifier) {
            Ok(s.buffered_amount_low)
        } else {
            Err(Error::ErrStreamClosed)
        }
    }

    /// set_buffered_amount_low_threshold is used to update the threshold.
    /// See buffered_amount_low_threshold().
    pub fn set_buffered_amount_low_threshold(&mut self, th: usize) -> Result<()> {
        if let Some(s) = self.association.streams.get_mut(&self.stream_identifier) {
            s.buffered_amount_low = th;
            Ok(())
        } else {
            Err(Error::ErrStreamClosed)
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RecvSendState {
    Closed = 0,
    Readable = 1,
    Writable = 2,
    ReadWritable = 3,
}

impl From<u8> for RecvSendState {
    fn from(v: u8) -> Self {
        match v {
            1 => RecvSendState::Readable,
            2 => RecvSendState::Writable,
            3 => RecvSendState::ReadWritable,
            _ => RecvSendState::Closed,
        }
    }
}

impl Default for RecvSendState {
    fn default() -> Self {
        RecvSendState::Closed
    }
}

/// StreamState represents the state of an SCTP stream
#[derive(Default, Debug)]
pub struct StreamState {
    pub(crate) side: Side,
    pub(crate) max_payload_size: u32,
    pub(crate) stream_identifier: StreamId,
    pub(crate) default_payload_type: PayloadProtocolIdentifier,
    pub(crate) reassembly_queue: ReassemblyQueue,
    pub(crate) sequence_number: u16,
    pub(crate) state: RecvSendState,
    pub(crate) unordered: bool,
    pub(crate) reliability_type: ReliabilityType,
    pub(crate) reliability_value: u32,
    pub(crate) buffered_amount: usize,
    pub(crate) buffered_amount_low: usize,
}
impl StreamState {
    pub(crate) fn new(
        side: Side,
        stream_identifier: StreamId,
        max_payload_size: u32,
        default_payload_type: PayloadProtocolIdentifier,
    ) -> Self {
        StreamState {
            side,
            stream_identifier,
            max_payload_size,
            default_payload_type,
            reassembly_queue: ReassemblyQueue::new(stream_identifier),
            sequence_number: 0,
            state: RecvSendState::ReadWritable,
            unordered: false,
            reliability_type: ReliabilityType::Reliable,
            reliability_value: 0,
            buffered_amount: 0,
            buffered_amount_low: 0,
        }
    }

    pub(crate) fn handle_data(&mut self, pd: &ChunkPayloadData) {
        self.reassembly_queue.push(pd.clone());
    }

    pub(crate) fn handle_forward_tsn_for_ordered(&mut self, ssn: u16) {
        if self.unordered {
            return; // unordered chunks are handled by handleForwardUnordered method
        }

        // Remove all chunks older than or equal to the new TSN from
        // the reassembly_queue.
        self.reassembly_queue.forward_tsn_for_ordered(ssn);
    }

    pub(crate) fn handle_forward_tsn_for_unordered(&mut self, new_cumulative_tsn: u32) {
        if !self.unordered {
            return; // ordered chunks are handled by handleForwardTSNOrdered method
        }

        // Remove all chunks older than or equal to the new TSN from
        // the reassembly_queue.
        self.reassembly_queue
            .forward_tsn_for_unordered(new_cumulative_tsn);
    }

    fn packetize(&mut self, raw: &Bytes, ppi: PayloadProtocolIdentifier) -> Vec<ChunkPayloadData> {
        let mut i = 0;
        let mut remaining = raw.len();

        // From draft-ietf-rtcweb-data-protocol-09, section 6:
        //   All Data Channel Establishment Protocol messages MUST be sent using
        //   ordered delivery and reliable transmission.
        let unordered = ppi != PayloadProtocolIdentifier::Dcep && self.unordered;

        let mut chunks = vec![];

        let head_abandoned = false;
        let head_all_inflight = false;
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
                abandoned: head_abandoned, // all fragmented chunks use the same abandoned
                all_inflight: head_all_inflight, // all fragmented chunks use the same all_inflight
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

        //let old_value = self.buffered_amount;
        self.buffered_amount += raw.len();
        //trace!("[{}] bufferedAmount = {}", self.side, old_value + raw.len());

        chunks
    }

    /// This method is called by association's read_loop (go-)routine to notify this stream
    /// of the specified amount of outgoing data has been delivered to the peer.
    pub(crate) fn on_buffer_released(&mut self, n_bytes_released: i64) -> bool {
        if n_bytes_released <= 0 {
            return false;
        }

        let from_amount = self.buffered_amount;
        let new_amount = if from_amount < n_bytes_released as usize {
            self.buffered_amount = 0;
            error!(
                "[{}] released buffer size {} should be <= {}",
                self.side, n_bytes_released, 0,
            );
            0
        } else {
            self.buffered_amount -= n_bytes_released as usize;

            from_amount - n_bytes_released as usize
        };

        let buffered_amount_low = self.buffered_amount_low;

        trace!(
            "[{}] bufferedAmount = {}, from_amount = {}, buffered_amount_low = {}",
            self.side,
            new_amount,
            from_amount,
            buffered_amount_low,
        );

        if from_amount > buffered_amount_low && new_amount <= buffered_amount_low {
            true
        } else {
            false
        }
    }

    pub(crate) fn get_num_bytes_in_reassembly_queue(&self) -> usize {
        // No lock is required as it reads the size with atomic load function.
        self.reassembly_queue.get_num_bytes()
    }
}
