use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_util::{io::AsyncRead, ready};
use proto::{AssociationError, Chunk, Chunks, ErrorCauseCode, StreamId};
use thiserror::Error;
use tokio::io::ReadBuf;

use crate::{association::AssociationRef, send_stream::UnknownStream};

/// A stream that can be used to send/receive data
#[derive(Debug)]
pub struct RecvStream {
    conn: AssociationRef,
    stream: StreamId,

    all_data_read: bool,
}

impl Drop for RecvStream {
    fn drop(&mut self) {
        let mut conn = self.conn.lock("RecvStream::drop");
        if conn.error.is_some() {
            return;
        }
        if !self.all_data_read {
            if let Ok(mut stream) = conn.inner.stream(self.stream) {
                let _ = stream.stop();
            }
            conn.wake();
        }
    }
}

impl RecvStream {
    pub(crate) fn new(conn: AssociationRef, stream: StreamId) -> Self {
        Self {
            conn,
            stream,

            all_data_read: false,
        }
    }

    /// stream_identifier returns the Stream identifier associated to the stream.
    pub fn stream_identifier(&self) -> StreamId {
        self.stream
    }

    /// Read data contiguously from the stream.
    ///
    /// Yields the number of bytes read into `buf` on success, or `None` if the stream was finished.
    pub fn read<'a>(&'a mut self, buf: &'a mut [u8]) -> Read<'a> {
        Read {
            stream: self,
            buf: ReadBuf::new(buf),
        }
    }

    /// Read an exact number of bytes contiguously from the stream.
    ///
    /// See [`read()`] for details.
    ///
    /// [`read()`]: Stream::read
    pub fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> ReadExact<'a> {
        ReadExact {
            stream: self,
            buf: ReadBuf::new(buf),
        }
    }

    fn poll_read(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), ReadError>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        self.poll_read_generic(cx, |chunks| {
            let mut read = false;
            loop {
                if buf.remaining() == 0 {
                    // We know `read` is `true` because `buf.remaining()` was not 0 before
                    return Some(());
                }

                match chunks.next(buf.remaining()) {
                    Some(chunk) => {
                        buf.put_slice(&chunk.bytes);
                        read = true;
                    }
                    None => return if read { Some(()) } else { None },
                }
            }
        })
        .map(|res| res.map(|_| ()))
    }

    /// Read the next segment of data
    ///
    /// Yields `None` if the stream was finished. Otherwise, yields a segment of data and its
    /// offset in the stream.
    /// Slightly more efficient than `read` due to not copying. Chunk boundaries do not correspond
    /// to peer writes, and hence cannot be used as framing.
    pub fn read_chunk(&mut self, max_length: usize) -> ReadChunk<'_> {
        ReadChunk {
            stream: self,
            max_length,
        }
    }

    /// Foundation of [`read_chunk()`]: Stream::read_chunk
    fn poll_read_chunk(
        &mut self,
        cx: &mut Context<'_>,
        max_length: usize,
    ) -> Poll<Result<Option<Chunk>, ReadError>> {
        self.poll_read_generic(cx, |chunks| chunks.next(max_length))
    }

    /// Read the next segments of data
    ///
    /// Fills `bufs` with the segments of data beginning immediately after the
    /// last data yielded by `read` or `read_chunk`, or `None` if the stream was
    /// finished.
    ///
    /// Slightly more efficient than `read` due to not copying. Chunk boundaries
    /// do not correspond to peer writes, and hence cannot be used as framing.
    pub fn read_chunks<'a>(&'a mut self, bufs: &'a mut [Bytes]) -> ReadChunks<'a> {
        ReadChunks { stream: self, bufs }
    }

    /// Foundation of [`read_chunks()`]: Stream::read_chunks
    fn poll_read_chunks(
        &mut self,
        cx: &mut Context<'_>,
        bufs: &mut [Bytes],
    ) -> Poll<Result<Option<usize>, ReadError>> {
        if bufs.is_empty() {
            return Poll::Ready(Ok(Some(0)));
        }

        self.poll_read_generic(cx, |chunks| {
            let mut read = 0;
            loop {
                if read >= bufs.len() {
                    // We know `read > 0` because `bufs` cannot be empty here
                    return Some(read);
                }

                match chunks.next(usize::MAX) {
                    Some(chunk) => {
                        bufs[read] = chunk.bytes;
                        read += 1;
                    }
                    None => return if read == 0 { None } else { Some(read) },
                }
            }
        })
    }

    /// Convenience method to read all remaining data into a buffer
    ///
    /// The returned future fails with [`ReadToEndError::TooLong`] if it's longer than `size_limit`
    /// bytes. Uses unordered reads to be more efficient than using `AsyncRead` would allow.
    /// `size_limit` should be set to limit worst-case memory use.
    ///
    /// If unordered reads have already been made, the resulting buffer may have gaps containing
    /// arbitrary data.
    ///
    /// [`ReadToEndError::TooLong`]: crate::ReadToEndError::TooLong
    pub fn read_to_end(self, size_limit: usize) -> ReadToEnd {
        ReadToEnd {
            stream: self,
            size_limit,
            read: Vec::new(),
            size: 0,
        }
    }

    /// Stop accepting data
    ///
    /// Discards unread data and notifies the peer to stop transmitting. Once stopped, further
    /// attempts to operate on a stream will yield `UnknownStream` errors.
    pub fn stop(&mut self, _error_code: ErrorCauseCode) -> Result<(), UnknownStream> {
        let mut conn = self.conn.lock("RecvStream::stop");
        conn.inner.stream(self.stream)?.stop()?; //error_code
        conn.wake();
        self.all_data_read = true;
        Ok(())
    }

    /// Get the identity of this stream
    pub fn id(&self) -> StreamId {
        self.stream
    }

    /// Handle common logic related to reading out of a receive stream
    ///
    /// This takes an `FnMut` closure that takes care of the actual reading process, matching
    /// the detailed read semantics for the calling function with a particular return type.
    /// The closure can read from the passed `&mut Chunks` and has to return the status after
    /// reading: the amount of data read, and the status after the final read call.
    fn poll_read_generic<T, U>(
        &mut self,
        cx: &mut Context<'_>,
        mut read_fn: T,
    ) -> Poll<Result<Option<U>, ReadError>>
    where
        T: FnMut(&mut Chunks) -> Option<U>,
    {
        let mut conn = self.conn.lock("Stream::poll_read");

        // If we stored an error during a previous call, return it now. This can happen if a
        // `read_fn` both wants to return data and also returns an error in its final stream status.

        let mut recv = conn.inner.stream(self.stream)?;
        let status = if let Some(mut chunks) = recv.read()? {
            read_fn(&mut chunks)
            /*TODO: if chunks.finalize().should_transmit() {
                conn.wake();
            }*/
        } else {
            None
        };

        match status {
            Some(read) => Poll::Ready(Ok(Some(read))),
            None => {
                if let Some(ref x) = conn.error {
                    return Poll::Ready(Err(ReadError::AssociationLost(x.clone())));
                }
                conn.blocked_readers.insert(self.stream, cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// Future produced by [`Stream::read_to_end()`].
///
/// [`Stream::read_to_end()`]: crate::Stream::read_to_end
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct ReadToEnd {
    stream: RecvStream,
    read: Vec<Chunk>,
    size: usize,
    size_limit: usize,
}

impl Future for ReadToEnd {
    type Output = Result<Vec<u8>, ReadToEndError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match ready!(self.stream.poll_read_chunk(cx, usize::MAX))? {
                Some(chunk) => {
                    self.size += chunk.bytes.len();
                    if self.size > self.size_limit {
                        return Poll::Ready(Err(ReadToEndError::TooLong));
                    }
                    self.read.push(chunk);
                }
                None => {
                    if self.size == 0 {
                        // Never received anything
                        return Poll::Ready(Ok(Vec::new()));
                    }
                    let mut buffer = Vec::with_capacity(self.size);
                    for data in self.read.drain(..) {
                        buffer.extend_from_slice(&data.bytes);
                    }
                    return Poll::Ready(Ok(buffer));
                }
            }
        }
    }
}

/// Error from the [`ReadToEnd`] future.
///
/// [`ReadToEnd`]: crate::ReadToEnd
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ReadToEndError {
    /// An error occurred during reading
    #[error("read error: {0}")]
    Read(#[from] ReadError),
    /// The stream is larger than the user-supplied limit
    #[error("stream too long")]
    TooLong,
}

impl AsyncRead for RecvStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let mut buf = ReadBuf::new(buf);
        ready!(RecvStream::poll_read(self.get_mut(), cx, &mut buf))?;
        Poll::Ready(Ok(buf.filled().len()))
    }
}

impl tokio::io::AsyncRead for RecvStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        ready!(RecvStream::poll_read(self.get_mut(), cx, buf))?;
        Poll::Ready(Ok(()))
    }
}

/// Errors that arise from reading from a stream.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The proto error
    #[error("proto error {0}")]
    Error(#[from] proto::Error),
    /// The peer abandoned transmitting data on this stream
    ///
    /// Carries an application-defined error code.
    #[error("stream reset by peer: error {0}")]
    Reset(ErrorCauseCode),
    /// The association was lost
    #[error("association lost")]
    AssociationLost(#[from] AssociationError),
    /// The stream has already been stopped, finished, or reset
    #[error("unknown stream")]
    UnknownStream,
    /// Attempted an ordered read following an unordered read
    ///
    /// Performing an unordered read allows discontinuities to arise in the receive buffer of a
    /// stream which cannot be recovered, making further ordered reads impossible.
    #[error("ordered read after unordered read")]
    IllegalOrderedRead,
}

impl From<ReadError> for io::Error {
    fn from(x: ReadError) -> Self {
        use self::ReadError::*;
        let kind = match x {
            Reset { .. } => io::ErrorKind::ConnectionReset,
            Error(_) | AssociationLost(_) | UnknownStream => io::ErrorKind::NotConnected,
            IllegalOrderedRead => io::ErrorKind::InvalidInput,
        };
        io::Error::new(kind, x)
    }
}

/// Future produced by [`Stream::read()`].
///
/// [`Stream::read()`]: crate::Stream::read
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct Read<'a> {
    stream: &'a mut RecvStream,
    buf: ReadBuf<'a>,
}

impl<'a> Future for Read<'a> {
    type Output = Result<Option<usize>, ReadError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        ready!(this.stream.poll_read(cx, &mut this.buf))?;
        match this.buf.filled().len() {
            0 if this.buf.capacity() != 0 => Poll::Ready(Ok(None)),
            n => Poll::Ready(Ok(Some(n))),
        }
    }
}

/// Future produced by [`Stream::read_exact()`].
///
/// [`Stream::read_exact()`]: crate::Stream::read_exact
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct ReadExact<'a> {
    stream: &'a mut RecvStream,
    buf: ReadBuf<'a>,
}

impl<'a> Future for ReadExact<'a> {
    type Output = Result<(), ReadExactError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut remaining = this.buf.remaining();
        while remaining > 0 {
            ready!(this.stream.poll_read(cx, &mut this.buf))?;
            let new = this.buf.remaining();
            if new == remaining {
                return Poll::Ready(Err(ReadExactError::FinishedEarly));
            }
            remaining = new;
        }
        Poll::Ready(Ok(()))
    }
}

/// Errors that arise from reading from a stream.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ReadExactError {
    /// The stream finished before all bytes were read
    #[error("stream finished early")]
    FinishedEarly,
    /// A read error occurred
    #[error(transparent)]
    ReadError(#[from] ReadError),
}

/// Future produced by [`Stream::read_chunk()`].
///
/// [`Stream::read_chunk()`]: crate::Stream::read_chunk
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct ReadChunk<'a> {
    stream: &'a mut RecvStream,
    max_length: usize,
}

impl<'a> Future for ReadChunk<'a> {
    type Output = Result<Option<Chunk>, ReadError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let max_length = self.max_length;
        self.stream.poll_read_chunk(cx, max_length)
    }
}

/// Future produced by [`Stream::read_chunks()`].
///
/// [`Stream::read_chunks()`]: crate::Stream::read_chunks
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct ReadChunks<'a> {
    stream: &'a mut RecvStream,
    bufs: &'a mut [Bytes],
}

impl<'a> Future for ReadChunks<'a> {
    type Output = Result<Option<usize>, ReadError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        this.stream.poll_read_chunks(cx, this.bufs)
    }
}
