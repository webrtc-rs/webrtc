use crate::chunk::chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier};
use crate::error::Error;
use crate::queue::reassembly_queue::ReassemblyQueue;

use bytes::Bytes;
use std::fmt;
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
pub struct Stream {
    //TODO: association         :*Association
    max_payload_size: usize,
    //lock                :sync.RWMutex
    stream_identifier: u16,
    default_payload_type: PayloadProtocolIdentifier,
    reassembly_queue: ReassemblyQueue,
    sequence_number: u16,
    read_notifier: Notify,
    read_err: Option<Error>,
    write_err: Option<Error>,
    unordered: bool,
    reliability_type: ReliabilityType,
    reliability_value: u32,
    buffered_amount: u64,
    buffered_amount_low: u64,
    on_buffered_amount_low: OnBufferedAmountLowFn,
    //log                 :logging.LeveledLogger
    name: String,
}

impl Stream {
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
    pub async fn write(&mut self, p: &[u8]) -> Result<usize, Error> {
        self.write_sctp(p, self.default_payload_type).await
    }

    // write_sctp writes len(p) bytes from p to the DTLS connection
    pub async fn write_sctp(
        &mut self,
        p: &[u8],
        _ppi: PayloadProtocolIdentifier,
    ) -> Result<usize, Error> {
        /*maxMessageSize := s.association.MaxMessageSize()
        if len(p) > int(maxMessageSize) {
            return 0, fmt.Errorf("%w: %v", errOutboundPacketTooLarge, math.MaxUint16)
        }

        switch s.association.getState() {
        case shutdownSent, shutdownAckSent, shutdownPending, shutdownReceived:
            s.lock.Lock()
            if s.write_err == nil {
                s.write_err = errStreamClosed
            }
            s.lock.Unlock()
        default:
        }

        s.lock.RLock()
        err = s.write_err
        s.lock.RUnlock()
        if err != nil {
            return 0, err
        }

        chunks := s.packetize(p, ppi)*/

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
        //var head *chunkPayloadData
        while remaining != 0 {
            let fragment_size = std::cmp::min(self.max_payload_size, remaining); //self.association.max_payload_size

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
                //head:                 head,
                ..Default::default()
            };

            //TODO: if head == nil {
            //    head = chunk
            // }

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
    /*
        // Close closes the write-direction of the stream.
        // Future calls to write are not permitted after calling Close.
        func (s *Stream) Close() error {
            if sid, isOpen := func() (uint16, bool) {
                s.lock.Lock()
                defer s.lock.Unlock()

                isOpen := true
                if s.write_err == nil {
                    s.write_err = errStreamClosed
                } else {
                    isOpen = false
                }

                if s.read_err == nil {
                    s.read_err = io.EOF
                } else {
                    isOpen = false
                }
                s.read_notifier.Broadcast() // broadcast regardless

                return s.stream_identifier, isOpen
            }(); isOpen {
                // Reset the outgoing stream
                // https://tools.ietf.org/html/rfc6525
                return s.association.sendResetRequest(sid)
            }

            return nil
        }

        // BufferedAmount returns the number of bytes of data currently queued to be sent over this stream.
        func (s *Stream) BufferedAmount() uint64 {
            s.lock.RLock()
            defer s.lock.RUnlock()

            return s.buffered_amount
        }

        // BufferedAmountLowThreshold returns the number of bytes of buffered outgoing data that is
        // considered "low." Defaults to 0.
        func (s *Stream) BufferedAmountLowThreshold() uint64 {
            s.lock.RLock()
            defer s.lock.RUnlock()

            return s.buffered_amount_low
        }

        // SetBufferedAmountLowThreshold is used to update the threshold.
        // See BufferedAmountLowThreshold().
        func (s *Stream) SetBufferedAmountLowThreshold(th uint64) {
            s.lock.Lock()
            defer s.lock.Unlock()

            s.buffered_amount_low = th
        }

        // OnBufferedAmountLow sets the callback handler which would be called when the number of
        // bytes of outgoing data buffered is lower than the threshold.
        func (s *Stream) OnBufferedAmountLow(f func()) {
            s.lock.Lock()
            defer s.lock.Unlock()

            s.on_buffered_amount_low = f
        }

        // This method is called by association's readLoop (go-)routine to notify this stream
        // of the specified amount of outgoing data has been delivered to the peer.
        func (s *Stream) onBufferReleased(nBytesReleased int) {
            if nBytesReleased <= 0 {
                return
            }

            s.lock.Lock()

            fromAmount := s.buffered_amount

            if s.buffered_amount < uint64(nBytesReleased) {
                s.buffered_amount = 0
                s.log.Errorf("[%s] released buffer size %d should be <= %d",
                    s.name, nBytesReleased, s.buffered_amount)
            } else {
                s.buffered_amount -= uint64(nBytesReleased)
            }

            s.log.Tracef("[%s] buffered_amount = %d", s.name, s.buffered_amount)

            if s.on_buffered_amount_low != nil && fromAmount > s.buffered_amount_low && s.buffered_amount <= s.buffered_amount_low {
                f := s.on_buffered_amount_low
                s.lock.Unlock()
                f()
                return
            }

            s.lock.Unlock()
        }
    */
    pub(crate) fn get_num_bytes_in_reassembly_queue(&self) -> usize {
        // No lock is required as it reads the size with atomic load function.
        self.reassembly_queue.get_num_bytes()
    }
}
