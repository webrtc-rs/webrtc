#[cfg(test)]
mod data_channel_test;

use crate::error::Result;
use crate::{
    error::Error, message::message_channel_ack::*, message::message_channel_open::*, message::*,
};

use sctp::{
    association::Association, chunk::chunk_payload_data::PayloadProtocolIdentifier, stream::*,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use util::marshal::*;

use bytes::{Buf, Bytes};
use derive_builder::Builder;
use std::fmt;
use std::io;
use std::net::Shutdown;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

const RECEIVE_MTU: usize = 8192;

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

/// DataChannel represents a data channel
#[derive(Debug, Default, Clone)]
pub struct DataChannel {
    pub config: Config,
    stream: Arc<Stream>,

    // stats
    messages_sent: Arc<AtomicUsize>,
    messages_received: Arc<AtomicUsize>,
    bytes_sent: Arc<AtomicUsize>,
    bytes_received: Arc<AtomicUsize>,
}

impl DataChannel {
    pub fn new(stream: Arc<Stream>, config: Config) -> Self {
        Self {
            config,
            stream,
            ..Default::default()
        }
    }

    /// Dial opens a data channels over SCTP
    pub async fn dial(
        association: &Arc<Association>,
        identifier: u16,
        config: Config,
    ) -> Result<Self> {
        let stream = association
            .open_stream(identifier, PayloadProtocolIdentifier::Binary)
            .await?;

        Self::client(stream, config).await
    }

    /// Accept is used to accept incoming data channels over SCTP
    pub async fn accept(association: &Arc<Association>, config: Config) -> Result<Self> {
        let stream = association
            .accept_stream()
            .await
            .ok_or(Error::ErrStreamClosed)?;

        stream.set_default_payload_type(PayloadProtocolIdentifier::Binary);

        Self::server(stream, config).await
    }

    /// Client opens a data channel over an SCTP stream
    pub async fn client(stream: Arc<Stream>, config: Config) -> Result<Self> {
        if !config.negotiated {
            let msg = Message::DataChannelOpen(DataChannelOpen {
                channel_type: config.channel_type,
                priority: config.priority,
                reliability_parameter: config.reliability_parameter,
                label: config.label.bytes().collect(),
                protocol: config.protocol.bytes().collect(),
            })
            .marshal()?;

            stream
                .write_sctp(&msg, PayloadProtocolIdentifier::Dcep)
                .await?;
        }
        Ok(DataChannel::new(stream, config))
    }

    /// Server accepts a data channel over an SCTP stream
    pub async fn server(stream: Arc<Stream>, mut config: Config) -> Result<Self> {
        let mut buf = vec![0u8; RECEIVE_MTU];

        let (n, ppi) = stream.read_sctp(&mut buf).await?;

        if ppi != PayloadProtocolIdentifier::Dcep {
            return Err(Error::InvalidPayloadProtocolIdentifier(ppi as u8));
        }

        let mut read_buf = &buf[..n];
        let msg = Message::unmarshal(&mut read_buf)?;

        if let Message::DataChannelOpen(dco) = msg {
            config.channel_type = dco.channel_type;
            config.priority = dco.priority;
            config.reliability_parameter = dco.reliability_parameter;
            config.label = String::from_utf8(dco.label)?;
            config.protocol = String::from_utf8(dco.protocol)?;
        } else {
            return Err(Error::InvalidMessageType(msg.message_type() as u8));
        };

        let data_channel = DataChannel::new(stream, config);

        data_channel.write_data_channel_ack().await?;
        data_channel.commit_reliability_params();

        Ok(data_channel)
    }

    /// Read reads a packet of len(p) bytes as binary data
    pub async fn read(&self, buf: &mut [u8]) -> Result<usize> {
        self.read_data_channel(buf).await.map(|(n, _)| n)
    }

    /// ReadDataChannel reads a packet of len(p) bytes
    pub async fn read_data_channel(&self, buf: &mut [u8]) -> Result<(usize, bool)> {
        loop {
            //TODO: add handling of cancel read_data_channel
            let (mut n, ppi) = match self.stream.read_sctp(buf).await {
                Ok((n, ppi)) => (n, ppi),
                Err(err) => {
                    // When the peer sees that an incoming stream was
                    // reset, it also resets its corresponding outgoing stream.
                    self.stream.shutdown(Shutdown::Both).await?;

                    return Err(err.into());
                }
            };

            let mut is_string = false;
            match ppi {
                PayloadProtocolIdentifier::Dcep => {
                    let mut data = &buf[..n];
                    match self.handle_dcep(&mut data).await {
                        Ok(()) => {}
                        Err(err) => {
                            log::error!("Failed to handle DCEP: {:?}", err);
                        }
                    }
                    continue;
                }
                PayloadProtocolIdentifier::String | PayloadProtocolIdentifier::StringEmpty => {
                    is_string = true;
                }
                _ => {}
            };

            match ppi {
                PayloadProtocolIdentifier::StringEmpty | PayloadProtocolIdentifier::BinaryEmpty => {
                    n = 0;
                }
                _ => {}
            };

            self.messages_received.fetch_add(1, Ordering::SeqCst);
            self.bytes_received.fetch_add(n, Ordering::SeqCst);

            return Ok((n, is_string));
        }
    }

    /// MessagesSent returns the number of messages sent
    pub fn messages_sent(&self) -> usize {
        self.messages_sent.load(Ordering::SeqCst)
    }

    /// MessagesReceived returns the number of messages received
    pub fn messages_received(&self) -> usize {
        self.messages_received.load(Ordering::SeqCst)
    }

    /// BytesSent returns the number of bytes sent
    pub fn bytes_sent(&self) -> usize {
        self.bytes_sent.load(Ordering::SeqCst)
    }

    /// BytesReceived returns the number of bytes received
    pub fn bytes_received(&self) -> usize {
        self.bytes_received.load(Ordering::SeqCst)
    }

    /// StreamIdentifier returns the Stream identifier associated to the stream.
    pub fn stream_identifier(&self) -> u16 {
        self.stream.stream_identifier()
    }

    async fn handle_dcep<B>(&self, data: &mut B) -> Result<()>
    where
        B: Buf,
    {
        let msg = Message::unmarshal(data)?;

        match msg {
            Message::DataChannelOpen(_) => {
                // Note: DATA_CHANNEL_OPEN message is handled inside Server() method.
                // Therefore, the message will not reach here.
                log::debug!("Received DATA_CHANNEL_OPEN");
                let _ = self.write_data_channel_ack().await?;
            }
            Message::DataChannelAck(_) => {
                log::debug!("Received DATA_CHANNEL_ACK");
                self.commit_reliability_params();
            }
        };

        Ok(())
    }

    /// Write writes len(p) bytes from p as binary data
    pub async fn write(&self, data: &Bytes) -> Result<usize> {
        self.write_data_channel(data, false).await
    }

    /// WriteDataChannel writes len(p) bytes from p
    pub async fn write_data_channel(&self, data: &Bytes, is_string: bool) -> Result<usize> {
        let data_len = data.len();

        // https://tools.ietf.org/html/draft-ietf-rtcweb-data-channel-12#section-6.6
        // SCTP does not support the sending of empty user messages.  Therefore,
        // if an empty message has to be sent, the appropriate PPID (WebRTC
        // String Empty or WebRTC Binary Empty) is used and the SCTP user
        // message of one zero byte is sent.  When receiving an SCTP user
        // message with one of these PPIDs, the receiver MUST ignore the SCTP
        // user message and process it as an empty message.
        let ppi = match (is_string, data_len) {
            (false, 0) => PayloadProtocolIdentifier::BinaryEmpty,
            (false, _) => PayloadProtocolIdentifier::Binary,
            (true, 0) => PayloadProtocolIdentifier::StringEmpty,
            (true, _) => PayloadProtocolIdentifier::String,
        };

        self.messages_sent.fetch_add(1, Ordering::SeqCst);
        self.bytes_sent.fetch_add(data_len, Ordering::SeqCst);

        if data_len == 0 {
            let _ = self
                .stream
                .write_sctp(&Bytes::from_static(&[0]), ppi)
                .await?;
            Ok(0)
        } else {
            Ok(self.stream.write_sctp(data, ppi).await?)
        }
    }

    async fn write_data_channel_ack(&self) -> Result<usize> {
        let ack = Message::DataChannelAck(DataChannelAck {}).marshal()?;
        Ok(self
            .stream
            .write_sctp(&ack, PayloadProtocolIdentifier::Dcep)
            .await?)
    }

    /// Close closes the DataChannel and the underlying SCTP stream.
    pub async fn close(&self) -> Result<()> {
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
        Ok(self.stream.shutdown(Shutdown::Both).await?)
    }

    /// BufferedAmount returns the number of bytes of data currently queued to be
    /// sent over this stream.
    pub fn buffered_amount(&self) -> usize {
        self.stream.buffered_amount()
    }

    /// BufferedAmountLowThreshold returns the number of bytes of buffered outgoing
    /// data that is considered "low." Defaults to 0.
    pub fn buffered_amount_low_threshold(&self) -> usize {
        self.stream.buffered_amount_low_threshold()
    }

    /// SetBufferedAmountLowThreshold is used to update the threshold.
    /// See BufferedAmountLowThreshold().
    pub fn set_buffered_amount_low_threshold(&self, threshold: usize) {
        self.stream.set_buffered_amount_low_threshold(threshold)
    }

    /// OnBufferedAmountLow sets the callback handler which would be called when the
    /// number of bytes of outgoing data buffered is lower than the threshold.
    pub async fn on_buffered_amount_low(&self, f: OnBufferedAmountLowFn) {
        self.stream.on_buffered_amount_low(f).await
    }

    fn commit_reliability_params(&self) {
        let (unordered, reliability_type) = match self.config.channel_type {
            ChannelType::Reliable => (false, ReliabilityType::Reliable),
            ChannelType::ReliableUnordered => (true, ReliabilityType::Reliable),
            ChannelType::PartialReliableRexmit => (false, ReliabilityType::Rexmit),
            ChannelType::PartialReliableRexmitUnordered => (true, ReliabilityType::Rexmit),
            ChannelType::PartialReliableTimed => (false, ReliabilityType::Timed),
            ChannelType::PartialReliableTimedUnordered => (true, ReliabilityType::Timed),
        };

        self.stream.set_reliability_params(
            unordered,
            reliability_type,
            self.config.reliability_parameter,
        );
    }
}

/// A wrapper around around [`DataChannel`], which implements [`AsyncRead`] and
/// [`AsyncWrite`].
///
/// Both `poll_read` and `poll_write` calls allocate temporary buffers, which results in an
/// additional overhead.
pub struct PollDataChannel {
    data_channel: Arc<DataChannel>,
    poll_stream: PollStream,
}

impl PollDataChannel {
    /// Constructs a new `PollDataChannel`.
    pub fn new(data_channel: Arc<DataChannel>) -> Self {
        let stream = data_channel.stream.clone();
        Self {
            data_channel,
            poll_stream: PollStream::new(stream),
        }
    }

    /// Get back the inner data_channel.
    pub fn into_inner(self) -> Arc<DataChannel> {
        self.data_channel
    }

    /// Obtain a clone of the inner data_channel.
    pub fn clone_inner(&self) -> Arc<DataChannel> {
        self.data_channel.clone()
    }

    /// MessagesSent returns the number of messages sent
    pub fn messages_sent(&self) -> usize {
        self.data_channel.messages_sent.load(Ordering::SeqCst)
    }

    /// MessagesReceived returns the number of messages received
    pub fn messages_received(&self) -> usize {
        self.data_channel.messages_received.load(Ordering::SeqCst)
    }

    /// BytesSent returns the number of bytes sent
    pub fn bytes_sent(&self) -> usize {
        self.data_channel.bytes_sent.load(Ordering::SeqCst)
    }

    /// BytesReceived returns the number of bytes received
    pub fn bytes_received(&self) -> usize {
        self.data_channel.bytes_received.load(Ordering::SeqCst)
    }

    /// StreamIdentifier returns the Stream identifier associated to the stream.
    pub fn stream_identifier(&self) -> u16 {
        self.poll_stream.stream_identifier()
    }

    /// BufferedAmount returns the number of bytes of data currently queued to be
    /// sent over this stream.
    pub fn buffered_amount(&self) -> usize {
        self.poll_stream.buffered_amount()
    }

    /// BufferedAmountLowThreshold returns the number of bytes of buffered outgoing
    /// data that is considered "low." Defaults to 0.
    pub fn buffered_amount_low_threshold(&self) -> usize {
        self.poll_stream.buffered_amount_low_threshold()
    }
}

impl AsyncRead for PollDataChannel {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.poll_stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for PollDataChannel {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.poll_stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.poll_stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.poll_stream).poll_shutdown(cx)
    }
}

impl Clone for PollDataChannel {
    fn clone(&self) -> PollDataChannel {
        PollDataChannel::new(self.clone_inner())
    }
}

impl fmt::Debug for PollDataChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PollDataChannel")
            .field("data_channel", &self.data_channel)
            .finish()
    }
}

impl AsRef<DataChannel> for PollDataChannel {
    fn as_ref(&self) -> &DataChannel {
        &*self.data_channel
    }
}
