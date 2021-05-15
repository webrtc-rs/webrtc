use crate::chunk::chunk_payload_data::PayloadProtocolIdentifier;
use crate::error::Error;
use crate::queue::reassembly_queue::ReassemblyQueue;

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

    /// set_default_payload_type sets the default payload type used by Write.
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
    /*
        func (s *Stream) handleData(pd *chunkPayloadData) {
            s.lock.Lock()
            defer s.lock.Unlock()

            var readable bool
            if s.reassembly_queue.push(pd) {
                readable = s.reassembly_queue.isReadable()
                s.log.Debugf("[%s] reassembly_queue readable=%v", s.name, readable)
                if readable {
                    s.log.Debugf("[%s] read_notifier.signal()", s.name)
                    s.read_notifier.Signal()
                    s.log.Debugf("[%s] read_notifier.signal() done", s.name)
                }
            }
        }

        func (s *Stream) handleForwardTSNForOrdered(ssn uint16) {
            var readable bool

            func() {
                s.lock.Lock()
                defer s.lock.Unlock()

                if s.unordered {
                    return // unordered chunks are handled by handleForwardUnordered method
                }

                // Remove all chunks older than or equal to the new TSN from
                // the reassembly_queue.
                s.reassembly_queue.forwardTSNForOrdered(ssn)
                readable = s.reassembly_queue.isReadable()
            }()

            // Notify the reader asynchronously if there's a data chunk to read.
            if readable {
                s.read_notifier.Signal()
            }
        }

        func (s *Stream) handleForwardTSNForUnordered(newCumulativeTSN uint32) {
            var readable bool

            func() {
                s.lock.Lock()
                defer s.lock.Unlock()

                if !s.unordered {
                    return // ordered chunks are handled by handleForwardTSNOrdered method
                }

                // Remove all chunks older than or equal to the new TSN from
                // the reassembly_queue.
                s.reassembly_queue.forwardTSNForUnordered(newCumulativeTSN)
                readable = s.reassembly_queue.isReadable()
            }()

            // Notify the reader asynchronously if there's a data chunk to read.
            if readable {
                s.read_notifier.Signal()
            }
        }

        // Write writes len(p) bytes from p with the default Payload Protocol Identifier
        func (s *Stream) Write(p []byte) (n int, err error) {
            return s.WriteSCTP(p, s.default_payload_type)
        }

        // WriteSCTP writes len(p) bytes from p to the DTLS connection
        func (s *Stream) WriteSCTP(p []byte, ppi PayloadProtocolIdentifier) (n int, err error) {
            maxMessageSize := s.association.MaxMessageSize()
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

            chunks := s.packetize(p, ppi)

            return len(p), s.association.sendPayloadData(chunks)
        }

        func (s *Stream) packetize(raw []byte, ppi PayloadProtocolIdentifier) []*chunkPayloadData {
            s.lock.Lock()
            defer s.lock.Unlock()

            i := uint32(0)
            remaining := uint32(len(raw))

            // From draft-ietf-rtcweb-data-protocol-09, section 6:
            //   All Data Channel Establishment Protocol messages MUST be sent using
            //   ordered delivery and reliable transmission.
            unordered := ppi != PayloadTypeWebRTCDCEP && s.unordered

            var chunks []*chunkPayloadData
            var head *chunkPayloadData
            for remaining != 0 {
                fragmentSize := min32(s.association.maxPayloadSize, remaining)

                // Copy the userdata since we'll have to store it until acked
                // and the caller may re-use the buffer in the mean time
                userData := make([]byte, fragmentSize)
                copy(userData, raw[i:i+fragmentSize])

                chunk := &chunkPayloadData{
                    stream_identifier:     s.stream_identifier,
                    userData:             userData,
                    unordered:            unordered,
                    beginningFragment:    i == 0,
                    endingFragment:       remaining-fragmentSize == 0,
                    immediateSack:        false,
                    payloadType:          ppi,
                    streamSequenceNumber: s.sequence_number,
                    head:                 head,
                }

                if head == nil {
                    head = chunk
                }

                chunks = append(chunks, chunk)

                remaining -= fragmentSize
                i += fragmentSize
            }

            // RFC 4960 Sec 6.6
            // Note: When transmitting ordered and unordered data, an endpoint does
            // not increment its Stream Sequence Number when transmitting a DATA
            // chunk with U flag set to 1.
            if !unordered {
                s.sequence_number++
            }

            s.buffered_amount += uint64(len(raw))
            s.log.Tracef("[%s] buffered_amount = %d", s.name, s.buffered_amount)

            return chunks
        }

        // Close closes the write-direction of the stream.
        // Future calls to Write are not permitted after calling Close.
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
