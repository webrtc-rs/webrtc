use std::{
    io::{Read, Write},
    sync::atomic::{AtomicUsize, Ordering},
};

use bytes::{Buf, Bytes, BytesMut};
use derive_builder::Builder;

use crate::{
    error::DataChannelError,
    exact_size_buf::ExactSizeBuf,
    marshal::{Marshal, Unmarshal},
    message::{DataChannelOpen, Message},
    sctp::{self, Association, PayloadType, StreamError},
    ChannelType,
};

const RECEIVE_MTU: usize = 8192;

/// Reader is an extended io.Reader
/// that also returns if the message is text.
trait ChannelReader: Read {
    fn read_data_channel(&mut self); // ([]byte) (int, bool, error)
}

/// Writer is an extended io.Writer
/// that also allows indicating if a message is text.
trait ChannelWriter: Write {
    fn write_data_channel(&mut self); // []byte, bool) (int, error)
}

/// ReadWriteCloser is an extended io.ReadWriteCloser
/// that also implements our Reader and Writer.
trait ChannelReadWriteCloser: ChannelReader + ChannelWriter {}

/// DataChannel represents a data channel
pub struct DataChannel {
    pub messages_sent: AtomicUsize,
    pub messages_received: AtomicUsize,
    pub bytes_sent: AtomicUsize,
    pub bytes_received: AtomicUsize,
    pub stream: sctp::Stream,
    pub config: Config,
}

impl DataChannel {
    pub fn new(stream: sctp::Stream, config: Config) -> Self {
        let messages_sent = AtomicUsize::new(0);
        let messages_received = AtomicUsize::new(0);
        let bytes_sent = AtomicUsize::new(0);
        let bytes_received = AtomicUsize::new(0);

        Self {
            messages_sent,
            messages_received,
            bytes_sent,
            bytes_received,
            stream,
            config,
        }
    }
}

/// Config is used to configure the data channel.
#[derive(Eq, PartialEq, Default, Clone, Debug, Builder)]
pub struct Config {
    #[builder(default)]
    pub channel_type: ChannelType,
    #[builder(default)]
    pub negotiated: bool,
    #[builder(default)]
    pub priority: u16,
    #[builder(default)]
    pub reliability_parameter: u32,
    #[builder(default)]
    pub label: String,
    #[builder(default)]
    pub protocol: String,
}

impl DataChannel {
    /// Dial opens a data channels over SCTP
    pub fn dial(
        association: &Association,
        identifier: u16,
        config: Config,
    ) -> Result<Self, DataChannelError> {
        let stream = association.open_stream(identifier, PayloadType::WebRtcBinary)?;

        Self::client(stream, config)
    }

    /// Accept is used to accept incoming data channels over SCTP
    pub fn accept(association: &Association, config: Config) -> Result<Self, DataChannelError> {
        let mut stream = association.accept_stream()?;

        stream.set_default_payload_type(PayloadType::WebRtcBinary);

        Self::server(stream, config)
    }

    /// Client opens a data channel over an SCTP stream
    pub fn client(mut stream: sctp::Stream, config: Config) -> Result<Self, DataChannelError> {
        if !config.negotiated {
            let open = Message::DataChannelOpen(DataChannelOpen {
                channel_type: config.channel_type,
                priority: config.priority,
                reliability_parameter: config.reliability_parameter,
                label: config.label.bytes().collect(),
                protocol: config.protocol.bytes().collect(),
            });
            let mut open_bytes = open.marshal()?;
            stream.write_sctp(&mut open_bytes, PayloadType::WebRtcDcep)?;
        }
        Ok(DataChannel::new(stream, config))
    }

    // Server accepts a data channel over an SCTP stream
    pub fn server(mut stream: sctp::Stream, mut config: Config) -> Result<Self, DataChannelError> {
        let mut buf = BytesMut::with_capacity(RECEIVE_MTU);

        let (n, ppi) = stream.read_sctp(&mut buf)?;

        if ppi != sctp::PayloadType::WebRtcDcep {
            return Err(DataChannelError::InvalidPayloadProtocolIdentifier {
                invalid_identifier: ppi,
            });
        }

        let mut buf = Bytes::copy_from_slice(buf.get(..n).unwrap());
        let open = Message::unmarshal_from(&mut buf)?;

        if let Message::DataChannelOpen(open) = open {
            config.channel_type = open.channel_type;
            config.priority = open.priority;
            config.reliability_parameter = open.reliability_parameter;
            config.label = String::from_utf8(open.label)?;
            config.protocol = String::from_utf8(open.protocol)?;
        } else {
            return Err(DataChannelError::InvalidMessageType {
                invalid_type: open.message_type(),
            });
        };

        let mut data_channel = DataChannel::new(stream, config);

        data_channel.write_data_channel_ack()?;

        data_channel.commit_reliability_params()?;

        Ok(data_channel)
    }

    /// Read reads a packet of len(p) bytes as binary data
    pub fn read(&mut self, buf: &mut BytesMut) -> Result<usize, DataChannelError> {
        self.read_data_channel(buf).map(|(n, _)| n)
    }

    /// ReadDataChannel reads a packet of len(p) bytes
    pub fn read_data_channel(
        &mut self,
        buf: &mut BytesMut,
    ) -> Result<(usize, bool), DataChannelError> {
        loop {
            let (n, ppi) = match self.stream.read_sctp(buf) {
                Ok((n, ppi)) => (n, ppi),
                Err(error @ StreamError::Eof) => {
                    // When the peer sees that an incoming stream was
                    // reset, it also resets its corresponding outgoing stream.
                    self.stream.close()?;

                    return Err(error.into());
                }
            };

            let bytes_len = match (n, &ppi) {
                (n, &PayloadType::WebRtcDcep) => {
                    let mut buf = Bytes::copy_from_slice(buf.get(..n).unwrap());
                    match self.handle_dcep(&mut buf) {
                        Ok(()) => {}
                        Err(error) => {
                            log::error!("Failed to handle DCEP: {:?}", error);
                        }
                    }
                    continue;
                }
                (_, ppi) if ppi.is_empty() => 0,
                (n, _) => n,
            };

            self.messages_received.fetch_add(1, Ordering::Relaxed);
            self.bytes_received.fetch_add(bytes_len, Ordering::Relaxed);

            let is_string = ppi.is_string();

            return Ok((bytes_len, is_string));
        }
    }

    /// MessagesSent returns the number of messages sent
    pub fn messages_sent(&self) -> usize {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// MessagesReceived returns the number of messages received
    pub fn messages_received(&self) -> usize {
        self.messages_received.load(Ordering::Relaxed)
    }

    /// BytesSent returns the number of bytes sent
    pub fn bytes_sent(&self) -> usize {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    /// BytesReceived returns the number of bytes received
    pub fn bytes_received(&self) -> usize {
        self.bytes_received.load(Ordering::Relaxed)
    }

    /// StreamIdentifier returns the Stream identifier associated to the stream.
    pub fn stream_identifier(&self) -> u16 {
        self.stream.stream_identifier()
    }

    pub fn handle_dcep<B>(&mut self, bytes: &mut B) -> Result<(), DataChannelError>
    where
        B: Buf,
    {
        let message = Message::unmarshal_from(bytes)?;

        match message {
            Message::DataChannelAck => {
                log::debug!("Received DATA_CHANNEL_ACK");

                self.commit_reliability_params()
            }
            message => Err(DataChannelError::InvalidMessageType {
                invalid_type: message.message_type(),
            }),
        }
    }

    /// Write writes len(p) bytes from p as binary data
    pub fn write<B>(&mut self, bytes: &mut B) -> Result<usize, DataChannelError>
    where
        B: Buf + ExactSizeBuf,
    {
        self.write_data_channel(bytes, false)
    }

    /// WriteDataChannel writes len(p) bytes from p
    pub fn write_data_channel<B>(
        &mut self,
        bytes: &mut B,
        is_string: bool,
    ) -> Result<usize, DataChannelError>
    where
        B: Buf + ExactSizeBuf,
    {
        let bytes_len = bytes.len();

        // https://tools.ietf.org/html/draft-ietf-rtcweb-data-channel-12#section-6.6
        // SCTP does not support the sending of empty user messages.  Therefore,
        // if an empty message has to be sent, the appropriate PPID (WebRTC
        // String Empty or WebRTC Binary Empty) is used and the SCTP user
        // message of one zero byte is sent.  When receiving an SCTP user
        // message with one of these PPIDs, the receiver MUST ignore the SCTP
        // user message and process it as an empty message.
        let ppi = match (is_string, bytes_len) {
            (false, 0) => sctp::PayloadType::WebRtcBinaryEmpty,
            (false, _) => sctp::PayloadType::WebRtcBinary,
            (true, 0) => sctp::PayloadType::WebRtcStringEmpty,
            (true, _) => sctp::PayloadType::WebRtcString,
        };

        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(bytes_len, Ordering::Relaxed);

        self.stream.write_sctp(bytes, ppi).map_err(From::from)
    }

    pub fn write_data_channel_ack(&mut self) -> Result<usize, DataChannelError> {
        let ack = Message::DataChannelAck;
        let mut ack_bytes = ack.marshal()?;

        self.stream
            .write_sctp(&mut ack_bytes, PayloadType::WebRtcDcep)
            .map_err(From::from)
    }

    /// Close closes the DataChannel and the underlying SCTP stream.
    pub fn close(&mut self) -> Result<(), DataChannelError> {
        // https://tools.ietf.org/html/draft-ietf-rtcweb-data-channel-13#section-6.7
        // Closing of a data channel MUST be signaled by resetting the
        // corresponding outgoing streams [RFC6525].  This means that if one
        // side decides to close the data channel, it resets the corresponding
        // outgoing stream.  When the peer sees that an incoming stream was
        // reset, it also resets its corresponding outgoing stream.  Once this
        // is completed, the data channel is closed.  Resetting a stream sets
        // the Stream Sequence Numbers (SSNs) of the stream back to 'zero' with
        // a corresponding notification to the application layer that the reset
        // has been performed.  Streams are available for reuse after a reset
        // has been performed.
        self.stream.close().map_err(From::from)
    }

    /// BufferedAmount returns the number of bytes of data currently queued to be
    /// sent over this stream.
    pub fn buffered_amount(&self) -> u64 {
        self.stream.buffered_amount()
    }

    /// BufferedAmountLowThreshold returns the number of bytes of buffered outgoing
    /// data that is considered "low." Defaults to 0.
    pub fn buffered_amount_low_threshold(&self) -> u64 {
        self.stream.buffered_amount_low_threshold()
    }

    /// SetBufferedAmountLowThreshold is used to update the threshold.
    /// See BufferedAmountLowThreshold().
    pub fn set_buffered_amount_low_threshold(&mut self, threshold: u64) {
        self.stream.set_buffered_amount_low_threshold(threshold)
    }

    /// OnBufferedAmountLow sets the callback handler which would be called when the
    /// number of bytes of outgoing data buffered is lower than the threshold.
    pub fn on_buffered_amount_low<F>(&mut self, f: F) {
        self.stream.on_buffered_amount_low(f)
    }

    pub fn commit_reliability_params(&mut self) -> Result<(), DataChannelError> {
        let (unordered, reliability_type) = match self.config.channel_type {
            ChannelType::Reliable => (false, sctp::ReliabilityType::Reliable),
            ChannelType::ReliableUnordered => (true, sctp::ReliabilityType::Reliable),
            ChannelType::PartialReliableRexmit => (false, sctp::ReliabilityType::Rexmit),
            ChannelType::PartialReliableRexmitUnordered => (true, sctp::ReliabilityType::Rexmit),
            ChannelType::PartialReliableTimed => (false, sctp::ReliabilityType::Timed),
            ChannelType::PartialReliableTimedUnordered => (true, sctp::ReliabilityType::Timed),
        };

        self.stream
            .set_reliability_params(
                unordered,
                reliability_type,
                self.config.reliability_parameter,
            )
            .map_err(From::from)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
