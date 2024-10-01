#[cfg(test)]
mod data_channel_test;

use std::borrow::Borrow;
use std::future::Future;
use std::net::Shutdown;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::{fmt, io};

use bytes::{Buf, Bytes};
use portable_atomic::AtomicUsize;
use sctp::association::Association;
use sctp::chunk::chunk_payload_data::PayloadProtocolIdentifier;
use sctp::stream::*;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use util::marshal::*;

use crate::error::{Error, Result};
use crate::message::message_channel_ack::*;
use crate::message::message_channel_open::*;
use crate::message::*;

const RECEIVE_MTU: usize = 8192;

/// Config is used to configure the data channel.
#[derive(Eq, PartialEq, Default, Clone, Debug)]
pub struct Config {
    pub channel_type: ChannelType,
    pub negotiated: bool,
    pub priority: u16,
    pub reliability_parameter: u32,
    pub label: String,
    pub protocol: String,
}

/// DataChannel represents a data channel
#[derive(Debug, Clone)]
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

            messages_sent: Arc::new(AtomicUsize::default()),
            messages_received: Arc::new(AtomicUsize::default()),
            bytes_sent: Arc::new(AtomicUsize::default()),
            bytes_received: Arc::new(AtomicUsize::default()),
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
    pub async fn accept<T>(
        association: &Arc<Association>,
        config: Config,
        existing_channels: &[T],
    ) -> Result<Self>
    where
        T: Borrow<Self>,
    {
        let stream = association
            .accept_stream()
            .await
            .ok_or(Error::ErrStreamClosed)?;

        for channel in existing_channels.iter().map(|ch| ch.borrow()) {
            if channel.stream_identifier() == stream.stream_identifier() {
                let ch = channel.to_owned();
                ch.stream
                    .set_default_payload_type(PayloadProtocolIdentifier::Binary);
                return Ok(ch);
            }
        }

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

    /// Read reads a packet of len(p) bytes as binary data.
    ///
    /// See [`sctp::stream::Stream::read_sctp`].
    pub async fn read(&self, buf: &mut [u8]) -> Result<usize> {
        self.read_data_channel(buf).await.map(|(n, _)| n)
    }

    /// ReadDataChannel reads a packet of len(p) bytes. It returns the number of bytes read and
    /// `true` if the data read is a string.
    ///
    /// See [`sctp::stream::Stream::read_sctp`].
    pub async fn read_data_channel(&self, buf: &mut [u8]) -> Result<(usize, bool)> {
        loop {
            //TODO: add handling of cancel read_data_channel
            let (mut n, ppi) = match self.stream.read_sctp(buf).await {
                Ok((0, PayloadProtocolIdentifier::Unknown)) => {
                    // The incoming stream was reset or the reading half was shutdown
                    return Ok((0, false));
                }
                Ok((n, ppi)) => (n, ppi),
                Err(err) => {
                    // Shutdown the stream and send the reset request to the remote.
                    self.close().await?;
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

        let n = if data_len == 0 {
            let _ = self
                .stream
                .write_sctp(&Bytes::from_static(&[0]), ppi)
                .await?;
            0
        } else {
            let n = self.stream.write_sctp(data, ppi).await?;
            self.bytes_sent.fetch_add(n, Ordering::SeqCst);
            n
        };

        self.messages_sent.fetch_add(1, Ordering::SeqCst);
        Ok(n)
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
    pub fn on_buffered_amount_low(&self, f: OnBufferedAmountLowFn) {
        self.stream.on_buffered_amount_low(f)
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

/// Default capacity of the temporary read buffer used by [`PollStream`].
const DEFAULT_READ_BUF_SIZE: usize = 8192;

/// State of the read `Future` in [`PollStream`].
enum ReadFut {
    /// Nothing in progress.
    Idle,
    /// Reading data from the underlying stream.
    Reading(Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send>>),
    /// Finished reading, but there's unread data in the temporary buffer.
    RemainingData(Vec<u8>),
}

impl ReadFut {
    /// Gets a mutable reference to the future stored inside `Reading(future)`.
    ///
    /// # Panics
    ///
    /// Panics if `ReadFut` variant is not `Reading`.
    fn get_reading_mut(&mut self) -> &mut Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send>> {
        match self {
            ReadFut::Reading(ref mut fut) => fut,
            _ => panic!("expected ReadFut to be Reading"),
        }
    }
}

/// A wrapper around around [`DataChannel`], which implements [`AsyncRead`] and
/// [`AsyncWrite`].
///
/// Both `poll_read` and `poll_write` calls allocate temporary buffers, which results in an
/// additional overhead.
pub struct PollDataChannel {
    data_channel: Arc<DataChannel>,

    read_fut: ReadFut,
    write_fut: Option<Pin<Box<dyn Future<Output = Result<usize>> + Send>>>,
    shutdown_fut: Option<Pin<Box<dyn Future<Output = Result<()>> + Send>>>,

    read_buf_cap: usize,
}

impl PollDataChannel {
    /// Constructs a new `PollDataChannel`.
    pub fn new(data_channel: Arc<DataChannel>) -> Self {
        Self {
            data_channel,
            read_fut: ReadFut::Idle,
            write_fut: None,
            shutdown_fut: None,
            read_buf_cap: DEFAULT_READ_BUF_SIZE,
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
        self.data_channel.messages_sent()
    }

    /// MessagesReceived returns the number of messages received
    pub fn messages_received(&self) -> usize {
        self.data_channel.messages_received()
    }

    /// BytesSent returns the number of bytes sent
    pub fn bytes_sent(&self) -> usize {
        self.data_channel.bytes_sent()
    }

    /// BytesReceived returns the number of bytes received
    pub fn bytes_received(&self) -> usize {
        self.data_channel.bytes_received()
    }

    /// StreamIdentifier returns the Stream identifier associated to the stream.
    pub fn stream_identifier(&self) -> u16 {
        self.data_channel.stream_identifier()
    }

    /// BufferedAmount returns the number of bytes of data currently queued to be
    /// sent over this stream.
    pub fn buffered_amount(&self) -> usize {
        self.data_channel.buffered_amount()
    }

    /// BufferedAmountLowThreshold returns the number of bytes of buffered outgoing
    /// data that is considered "low." Defaults to 0.
    pub fn buffered_amount_low_threshold(&self) -> usize {
        self.data_channel.buffered_amount_low_threshold()
    }

    /// Set the capacity of the temporary read buffer (default: 8192).
    pub fn set_read_buf_capacity(&mut self, capacity: usize) {
        self.read_buf_cap = capacity
    }
}

impl AsyncRead for PollDataChannel {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        let fut = match self.read_fut {
            ReadFut::Idle => {
                // read into a temporary buffer because `buf` has an unonymous lifetime, which can
                // be shorter than the lifetime of `read_fut`.
                let data_channel = self.data_channel.clone();
                let mut temp_buf = vec![0; self.read_buf_cap];
                self.read_fut = ReadFut::Reading(Box::pin(async move {
                    data_channel.read(temp_buf.as_mut_slice()).await.map(|n| {
                        temp_buf.truncate(n);
                        temp_buf
                    })
                }));
                self.read_fut.get_reading_mut()
            }
            ReadFut::Reading(ref mut fut) => fut,
            ReadFut::RemainingData(ref mut data) => {
                let remaining = buf.remaining();
                let len = std::cmp::min(data.len(), remaining);
                buf.put_slice(&data[..len]);
                if data.len() > remaining {
                    // ReadFut remains to be RemainingData
                    data.drain(..len);
                } else {
                    self.read_fut = ReadFut::Idle;
                }
                return Poll::Ready(Ok(()));
            }
        };

        loop {
            match fut.as_mut().poll(cx) {
                Poll::Pending => return Poll::Pending,
                // retry immediately upon empty data or incomplete chunks
                // since there's no way to setup a waker.
                Poll::Ready(Err(Error::Sctp(sctp::Error::ErrTryAgain))) => {}
                // EOF has been reached => don't touch buf and just return Ok
                Poll::Ready(Err(Error::Sctp(sctp::Error::ErrEof))) => {
                    self.read_fut = ReadFut::Idle;
                    return Poll::Ready(Ok(()));
                }
                Poll::Ready(Err(e)) => {
                    self.read_fut = ReadFut::Idle;
                    return Poll::Ready(Err(e.into()));
                }
                Poll::Ready(Ok(mut temp_buf)) => {
                    let remaining = buf.remaining();
                    let len = std::cmp::min(temp_buf.len(), remaining);
                    buf.put_slice(&temp_buf[..len]);
                    if temp_buf.len() > remaining {
                        temp_buf.drain(..len);
                        self.read_fut = ReadFut::RemainingData(temp_buf);
                    } else {
                        self.read_fut = ReadFut::Idle;
                    }
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl AsyncWrite for PollDataChannel {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        if let Some(fut) = self.write_fut.as_mut() {
            match fut.as_mut().poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(e)) => {
                    let data_channel = self.data_channel.clone();
                    let bytes = Bytes::copy_from_slice(buf);
                    self.write_fut =
                        Some(Box::pin(async move { data_channel.write(&bytes).await }));
                    Poll::Ready(Err(e.into()))
                }
                // Given the data is buffered, it's okay to ignore the number of written bytes.
                //
                // TODO: In the long term, `data_channel.write` should be made sync. Then we could
                // remove the whole `if` condition and just call `data_channel.write`.
                Poll::Ready(Ok(_)) => {
                    let data_channel = self.data_channel.clone();
                    let bytes = Bytes::copy_from_slice(buf);
                    self.write_fut =
                        Some(Box::pin(async move { data_channel.write(&bytes).await }));
                    Poll::Ready(Ok(buf.len()))
                }
            }
        } else {
            let data_channel = self.data_channel.clone();
            let bytes = Bytes::copy_from_slice(buf);
            let fut = self
                .write_fut
                .insert(Box::pin(async move { data_channel.write(&bytes).await }));

            match fut.as_mut().poll(cx) {
                // If it's the first time we're polling the future, `Poll::Pending` can't be
                // returned because that would mean the `PollDataChannel` is not ready for writing.
                // And this is not true since we've just created a future, which is going to write
                // the buf to the underlying stream.
                //
                // It's okay to return `Poll::Ready` if the data is buffered (this is what the
                // buffered writer and `File` do).
                Poll::Pending => Poll::Ready(Ok(buf.len())),
                Poll::Ready(Err(e)) => {
                    self.write_fut = None;
                    Poll::Ready(Err(e.into()))
                }
                Poll::Ready(Ok(n)) => {
                    self.write_fut = None;
                    Poll::Ready(Ok(n))
                }
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.write_fut.as_mut() {
            Some(fut) => match fut.as_mut().poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(e)) => {
                    self.write_fut = None;
                    Poll::Ready(Err(e.into()))
                }
                Poll::Ready(Ok(_)) => {
                    self.write_fut = None;
                    Poll::Ready(Ok(()))
                }
            },
            None => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.as_mut().poll_flush(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(_) => {}
        }

        let fut = match self.shutdown_fut.as_mut() {
            Some(fut) => fut,
            None => {
                let data_channel = self.data_channel.clone();
                self.shutdown_fut.get_or_insert(Box::pin(async move {
                    data_channel
                        .stream
                        .shutdown(Shutdown::Write)
                        .await
                        .map_err(Error::Sctp)
                }))
            }
        };

        match fut.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => {
                self.shutdown_fut = None;
                Poll::Ready(Err(e.into()))
            }
            Poll::Ready(Ok(_)) => {
                self.shutdown_fut = None;
                Poll::Ready(Ok(()))
            }
        }
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
            .field("read_buf_cap", &self.read_buf_cap)
            .finish()
    }
}

impl AsRef<DataChannel> for PollDataChannel {
    fn as_ref(&self) -> &DataChannel {
        &self.data_channel
    }
}
