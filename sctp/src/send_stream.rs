use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_channel::oneshot;
use futures_util::{io::AsyncWrite, ready, FutureExt};
use proto::{
    AssociationError, ErrorCauseCode, PayloadProtocolIdentifier, ReliabilityType, StreamId,
};
use thiserror::Error;

use crate::association::AssociationRef;

/// A stream that can be used to send/receive data
#[derive(Debug)]
pub struct SendStream {
    conn: AssociationRef,
    stream: StreamId,

    finishing: Option<oneshot::Receiver<Option<WriteError>>>,
}

impl Drop for SendStream {
    fn drop(&mut self) {
        let mut conn = self.conn.lock("SendStream::drop");
        if conn.error.is_some() {
            return;
        }
        if self.finishing.is_none() {
            if let Ok(mut stream) = conn.inner.stream(self.stream) {
                let _ = stream.finish();
            }
            conn.wake();
        }
    }
}

impl SendStream {
    pub(crate) fn new(conn: AssociationRef, stream: StreamId) -> Self {
        Self {
            conn,
            stream,

            finishing: None,
        }
    }

    /// stream_identifier returns the Stream identifier associated to the stream.
    pub fn stream_identifier(&self) -> StreamId {
        self.stream
    }

    /// set_default_payload_type sets the default payload type used by write.
    pub fn set_default_payload_type(
        &mut self,
        default_payload_type: PayloadProtocolIdentifier,
    ) -> Result<(), UnknownStream> {
        let mut conn = self.conn.lock("Stream::set_default_payload_type");
        Ok(conn
            .inner
            .stream(self.stream)?
            .set_default_payload_type(default_payload_type)?)
    }

    /// get_default_payload_type returns the payload type associated to the stream.
    pub fn get_default_payload_type(&self) -> Result<PayloadProtocolIdentifier, UnknownStream> {
        let mut conn = self.conn.lock("Stream::get_default_payload_type");
        Ok(conn.inner.stream(self.stream)?.get_default_payload_type()?)
    }

    /// set_reliability_params sets reliability parameters for this stream.
    pub fn set_reliability_params(
        &mut self,
        unordered: bool,
        rel_type: ReliabilityType,
        rel_val: u32,
    ) -> Result<(), UnknownStream> {
        let mut conn = self.conn.lock("Stream::set_reliability_params");
        Ok(conn
            .inner
            .stream(self.stream)?
            .set_reliability_params(unordered, rel_type, rel_val)?)
    }

    /// buffered_amount returns the number of bytes of data currently queued to be sent over this stream.
    pub fn buffered_amount(&self) -> Result<usize, UnknownStream> {
        let mut conn = self.conn.lock("Stream::buffered_amount");
        Ok(conn.inner.stream(self.stream)?.buffered_amount()?)
    }

    /// buffered_amount_low_threshold returns the number of bytes of buffered outgoing data that is
    /// considered "low." Defaults to 0.
    pub fn buffered_amount_low_threshold(&self) -> Result<usize, UnknownStream> {
        let mut conn = self.conn.lock("Stream::buffered_amount_low_threshold");
        Ok(conn
            .inner
            .stream(self.stream)?
            .buffered_amount_low_threshold()?)
    }

    /// set_buffered_amount_low_threshold is used to update the threshold.
    /// See buffered_amount_low_threshold().
    pub fn set_buffered_amount_low_threshold(&mut self, th: usize) -> Result<(), UnknownStream> {
        let mut conn = self.conn.lock("Stream::set_buffered_amount_low_threshold");
        Ok(conn
            .inner
            .stream(self.stream)?
            .set_buffered_amount_low_threshold(th)?)
    }
}

// Send part
impl SendStream {
    /// Write bytes to the stream
    ///
    /// Yields the number of bytes written on success. Congestion and flow control may cause this to
    /// be shorter than `buf.len()`, indicating that only a prefix of `buf` was written.
    pub fn write<'a>(&'a mut self, buf: &'a [u8]) -> Write<'a> {
        Write { stream: self, buf }
    }

    /// Convenience method to write an entire buffer to the stream
    pub fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> WriteAll<'a> {
        WriteAll { stream: self, buf }
    }

    /// Write chunks to the stream
    ///
    /// Yields the number of bytes and chunks written on success.
    /// Congestion and flow control may cause this to be shorter than `buf.len()`,
    /// indicating that only a prefix of `bufs` was written
    pub fn write_chunks<'a>(&'a mut self, bufs: &'a mut [Bytes]) -> WriteChunks<'a> {
        WriteChunks { stream: self, bufs }
    }

    /// Convenience method to write a single chunk in its entirety to the stream
    pub fn write_chunk(&mut self, buf: Bytes) -> WriteChunk<'_> {
        WriteChunk {
            stream: self,
            buf: [buf],
        }
    }

    /// Convenience method to write an entire list of chunks to the stream
    pub fn write_all_chunks<'a>(&'a mut self, bufs: &'a mut [Bytes]) -> WriteAllChunks<'a> {
        WriteAllChunks {
            stream: self,
            bufs,
            offset: 0,
        }
    }

    fn execute_poll<F, R>(
        &mut self,
        _cx: &mut Context<'_>,
        write_fn: F,
    ) -> Poll<Result<R, WriteError>>
    where
        F: FnOnce(&mut proto::Stream<'_>) -> Result<R, WriteError>,
    {
        let mut conn = self.conn.lock("Stream::poll_write");

        if let Some(ref x) = conn.error {
            return Poll::Ready(Err(WriteError::AssociationLost(x.clone())));
        }

        let result = match write_fn(&mut conn.inner.stream(self.stream)?) {
            Ok(result) => result,
            Err(error) => {
                return Poll::Ready(Err(error));
            }
        };

        conn.wake();
        Poll::Ready(Ok(result))
    }

    /// Shut down the send stream gracefully.
    ///
    /// No new data may be written after calling this method. Completes when the peer has
    /// acknowledged all sent data, retransmitting data as needed.
    pub fn finish(&mut self) -> Result<(), UnknownStream> {
        let mut conn = self.conn.lock("SendStream::stop");
        conn.inner.stream(self.stream)?.finish()?; //error_code
        conn.wake();
        //self.all_data_read = true;
        Ok(())
    }

    /*
    /// Shut down the send stream gracefully.
    ///
    /// No new data may be written after calling this method. Completes when the peer has
    /// acknowledged all sent data, retransmitting data as needed.
    pub fn finish(&mut self) -> Finish<'_> {
        Finish { stream: self }
    }*/

    #[doc(hidden)]
    pub fn poll_finish(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), WriteError>> {
        let mut conn = self.conn.lock("poll_finish");

        if self.finishing.is_none() {
            conn.inner.stream(self.stream)?.finish()?;
            let (send, recv) = oneshot::channel();
            self.finishing = Some(recv);
            conn.finishing.insert(self.stream, send);
            conn.wake();
        }
        match self
            .finishing
            .as_mut()
            .unwrap()
            .poll_unpin(cx)
            .map(|x| x.unwrap())
        {
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Ready(Some(e)) => Poll::Ready(Err(e)),
            Poll::Pending => {
                // To ensure that finished streams can be detected even after the association is
                // closed, we must only check for association errors after determining that the
                // stream has not yet been finished. Note that this relies on holding the association
                // lock so that it is impossible for the stream to become finished between the above
                // poll call and this check.
                if let Some(ref x) = conn.error {
                    return Poll::Ready(Err(WriteError::AssociationLost(x.clone())));
                }
                Poll::Pending
            }
        }
    }

    /// Completes if/when the peer stops the stream, yielding the error code
    pub fn stopped(&mut self) -> Stopped<'_> {
        Stopped { stream: self }
    }

    #[doc(hidden)]
    pub fn poll_stopped(&mut self, cx: &mut Context<'_>) -> Poll<Result<bool, StoppedError>> {
        let mut conn = self.conn.lock("SendStream::poll_stopped");

        if !conn.inner.stream(self.stream)?.is_writable() {
            Poll::Ready(Ok(true))
        } else {
            conn.stopped.insert(self.stream, cx.waker().clone());
            Poll::Pending
        }
    }
}

impl AsyncWrite for SendStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        SendStream::execute_poll(self.get_mut(), cx, |stream| {
            stream.write(buf).map_err(Into::into)
        })
        .map_err(Into::into)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_mut().poll_finish(cx).map_err(Into::into)
    }
}

impl tokio::io::AsyncWrite for SendStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write(self, cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        AsyncWrite::poll_close(self, cx)
    }
}

/// Future produced by `Stream::finish`
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct Finish<'a> {
    stream: &'a mut SendStream,
}

impl Future for Finish<'_> {
    type Output = Result<(), WriteError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.get_mut().stream.poll_finish(cx)
    }
}

/// Future produced by `Stream::stopped`
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct Stopped<'a> {
    stream: &'a mut SendStream,
}

impl Future for Stopped<'_> {
    type Output = Result<bool, StoppedError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.get_mut().stream.poll_stopped(cx)
    }
}

/// Future produced by [`Stream::write()`].
///
/// [`Stream::write()`]: crate::Stream::write
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct Write<'a> {
    stream: &'a mut SendStream,
    buf: &'a [u8],
}

impl<'a> Future for Write<'a> {
    type Output = Result<usize, WriteError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let buf = this.buf;
        this.stream
            .execute_poll(cx, |s| s.write(buf).map_err(Into::into))
    }
}

/// Future produced by [`Stream::write_all()`].
///
/// [`Stream::write_all()`]: crate::Stream::write_all
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct WriteAll<'a> {
    stream: &'a mut SendStream,
    buf: &'a [u8],
}

impl<'a> Future for WriteAll<'a> {
    type Output = Result<(), WriteError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        loop {
            if this.buf.is_empty() {
                return Poll::Ready(Ok(()));
            }
            let buf = this.buf;
            let n = ready!(this
                .stream
                .execute_poll(cx, |s| s.write(buf).map_err(Into::into)))?;
            this.buf = &this.buf[n..];
        }
    }
}

/// Future produced by [`Stream::write_chunks()`].
///
/// [`Stream::write_chunks()`]: crate::Stream::write_chunks
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct WriteChunks<'a> {
    stream: &'a mut SendStream,
    bufs: &'a mut [Bytes],
}

impl<'a> Future for WriteChunks<'a> {
    type Output = Result<usize, WriteError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let bufs = &mut *this.bufs;
        this.stream
            .execute_poll(cx, |s| s.write_chunks(bufs).map_err(Into::into))
    }
}

/// Future produced by [`Stream::write_chunk()`].
///
/// [`Stream::write_chunk()`]: crate::Stream::write_chunk
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct WriteChunk<'a> {
    stream: &'a mut SendStream,
    buf: [Bytes; 1],
}

impl<'a> Future for WriteChunk<'a> {
    type Output = Result<(), WriteError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        loop {
            if this.buf[0].is_empty() {
                return Poll::Ready(Ok(()));
            }
            let bufs = &mut this.buf[..];
            ready!(this
                .stream
                .execute_poll(cx, |s| s.write_chunks(bufs).map_err(Into::into)))?;
        }
    }
}

/// Future produced by [`Stream::write_all_chunks()`].
///
/// [`Stream::write_all_chunks()`]: crate::Stream::write_all_chunks
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct WriteAllChunks<'a> {
    stream: &'a mut SendStream,
    bufs: &'a mut [Bytes],
    offset: usize,
}

impl<'a> Future for WriteAllChunks<'a> {
    type Output = Result<(), WriteError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        loop {
            if this.offset == this.bufs.len() {
                return Poll::Ready(Ok(()));
            }
            let bufs = &mut this.bufs[this.offset..];
            let written = ready!(this
                .stream
                .execute_poll(cx, |s| s.write_chunks(bufs).map_err(Into::into)))?;
            this.offset += written;
        }
    }
}

/// Errors that arise from writing to a stream
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// The proto error
    #[error("proto error {0}")]
    Error(#[from] proto::Error),
    /// The peer is no longer accepting data on this stream
    ///
    /// Carries an application-defined error code.
    #[error("sending stopped by peer: error {0}")]
    Stopped(ErrorCauseCode),
    /// The association was lost
    #[error("association lost")]
    AssociationLost(#[from] AssociationError),
    /// The stream has already been finished or reset
    #[error("unknown stream")]
    UnknownStream,
}

impl From<WriteError> for io::Error {
    fn from(x: WriteError) -> Self {
        use self::WriteError::*;
        let kind = match x {
            Stopped(_) => io::ErrorKind::ConnectionReset,
            Error(_) | AssociationLost(_) | UnknownStream => io::ErrorKind::NotConnected,
        };
        io::Error::new(kind, x)
    }
}

/// Errors that arise while monitoring for a send stream stop from the peer
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StoppedError {
    /// The proto error
    #[error("proto error {0}")]
    Error(#[from] proto::Error),
    /// The association was lost
    #[error("association lost")]
    AssociationLost(#[from] AssociationError),
    /// The stream has already been finished or reset
    #[error("unknown stream")]
    UnknownStream,
}

/// Error indicating that a stream has already been finished or reset
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("unknown stream")]
pub struct UnknownStream {
    _private: (),
}

impl From<proto::Error> for UnknownStream {
    fn from(_: proto::Error) -> Self {
        UnknownStream { _private: () }
    }
}
