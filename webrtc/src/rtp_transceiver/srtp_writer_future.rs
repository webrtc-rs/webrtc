use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use bytes::Bytes;
use interceptor::{Attributes, RTCPReader, RTPWriter};
use portable_atomic::AtomicBool;
use srtp::session::Session;
use srtp::stream::Stream;
use tokio::sync::Mutex;
use util;

use crate::dtls_transport::RTCDtlsTransport;
use crate::error::{Error, Result};
use crate::rtp_transceiver::rtp_sender::RTPSenderInternal;
use crate::rtp_transceiver::SSRC;

/// `RTP` packet sequence number manager.
///
/// Used to override outgoing `RTP` packets' sequence numbers. On creating it is
/// unabled and can be enabled before sending data beginning. Once data sending
/// began it can not be enabled any more.
pub(crate) struct SequenceTransformer(util::sync::Mutex<SequenceTransformerInner>);

/// [`SequenceTransformer`] inner.
struct SequenceTransformerInner {
    offset: u16,
    last_sq: u16,
    reset_needed: bool,
    enabled: bool,
    data_sent: bool,
}

impl SequenceTransformer {
    /// Creates a new [`SequenceTransformer`].
    pub(crate) fn new() -> Self {
        Self(util::sync::Mutex::new(SequenceTransformerInner {
            offset: 0,
            last_sq: rand::random(),
            reset_needed: false,
            enabled: false,
            data_sent: false,
        }))
    }

    /// Enables this [`SequenceTransformer`].
    ///
    /// # Errors
    ///
    /// With [`Error::ErrRTPSenderSeqTransEnabled`] on trying to enable already
    /// enabled [`SequenceTransformer`].
    ///
    /// With [`Error::ErrRTPSenderSeqTransEnabled`] on trying to enable
    /// [`SequenceTransformer`] after data sending began.
    pub(crate) fn enable(&self) -> Result<()> {
        let mut guard = self.0.lock();

        if guard.enabled {
            return Err(Error::ErrRTPSenderSeqTransEnabled);
        }

        (!guard.data_sent)
            .then(|| {
                guard.enabled = true;
            })
            .ok_or(Error::ErrRTPSenderDataSent)
    }

    /// Indicates [`SequenceTransformer`] about necessity of recalculating
    /// `offset`.
    pub(crate) fn reset_offset(&self) {
        self.0.lock().reset_needed = true;
    }

    /// Gets [`Some`] consistent `sequence number` if this [`SequenceTransformer`] is
    /// enabled or [`None`] if it is not.
    ///
    /// Once this method is called, considers data sending began.
    fn seq_number(&self, raw_sn: u16) -> Option<u16> {
        let mut guard = self.0.lock();
        guard.data_sent = true;

        if !guard.enabled {
            return None;
        }

        let offset = guard
            .reset_needed
            .then(|| {
                guard.reset_needed = false;
                let offset = guard.last_sq.overflowing_sub(raw_sn.overflowing_sub(1).0).0;
                guard.offset = offset;
                offset
            })
            .unwrap_or(guard.offset);
        let next = raw_sn.overflowing_add(offset).0;
        guard.last_sq = next;

        Some(next)
    }
}

/// SrtpWriterFuture blocks Read/Write calls until
/// the SRTP Session is available
pub(crate) struct SrtpWriterFuture {
    pub(crate) closed: AtomicBool,
    pub(crate) ssrc: SSRC,
    pub(crate) rtp_sender: Weak<RTPSenderInternal>,
    pub(crate) rtp_transport: Arc<RTCDtlsTransport>,
    pub(crate) rtcp_read_stream: Mutex<Option<Arc<Stream>>>, // atomic.Value // *
    pub(crate) rtp_write_session: Mutex<Option<Arc<Session>>>, // atomic.Value // *
    pub(crate) seq_trans: Arc<SequenceTransformer>,
}

impl SrtpWriterFuture {
    async fn init(&self, return_when_no_srtp: bool) -> Result<()> {
        if return_when_no_srtp {
            {
                if let Some(rtp_sender) = self.rtp_sender.upgrade() {
                    if rtp_sender.stop_called_signal.load(Ordering::SeqCst) {
                        return Err(Error::ErrClosedPipe);
                    }
                } else {
                    return Err(Error::ErrClosedPipe);
                }
            }

            if !self.rtp_transport.srtp_ready_signal.load(Ordering::SeqCst) {
                return Ok(());
            }
        } else {
            let mut rx = self.rtp_transport.srtp_ready_rx.lock().await;
            if let Some(srtp_ready_rx) = &mut *rx {
                if let Some(rtp_sender) = self.rtp_sender.upgrade() {
                    tokio::select! {
                        _ = rtp_sender.stop_called_rx.notified()=> return Err(Error::ErrClosedPipe),
                        _ = srtp_ready_rx.recv() =>{}
                    }
                } else {
                    return Err(Error::ErrClosedPipe);
                }
            }
        }

        if self.closed.load(Ordering::SeqCst) {
            return Err(Error::ErrClosedPipe);
        }

        if let Some(srtcp_session) = self.rtp_transport.get_srtcp_session().await {
            let rtcp_read_stream = srtcp_session.open(self.ssrc).await;
            let mut stream = self.rtcp_read_stream.lock().await;
            *stream = Some(rtcp_read_stream);
        }

        {
            let srtp_session = self.rtp_transport.get_srtp_session().await;
            let mut session = self.rtp_write_session.lock().await;
            *session = srtp_session;
        }

        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.closed.store(true, Ordering::SeqCst);

        let stream = {
            let mut stream = self.rtcp_read_stream.lock().await;
            stream.take()
        };
        if let Some(rtcp_read_stream) = stream {
            Ok(rtcp_read_stream.close().await?)
        } else {
            Ok(())
        }
    }

    pub async fn read(&self, b: &mut [u8]) -> Result<usize> {
        {
            let stream = {
                let stream = self.rtcp_read_stream.lock().await;
                stream.clone()
            };
            if let Some(rtcp_read_stream) = stream {
                return Ok(rtcp_read_stream.read(b).await?);
            }
        }

        self.init(false).await?;

        {
            let stream = {
                let stream = self.rtcp_read_stream.lock().await;
                stream.clone()
            };
            if let Some(rtcp_read_stream) = stream {
                return Ok(rtcp_read_stream.read(b).await?);
            }
        }

        Ok(0)
    }

    pub async fn write_rtp(&self, pkt: &rtp::packet::Packet) -> Result<usize> {
        {
            let session = {
                let session = self.rtp_write_session.lock().await;
                session.clone()
            };
            if let Some(rtp_write_session) = session {
                return Ok(rtp_write_session.write_rtp(pkt).await?);
            }
        }

        self.init(true).await?;

        {
            let session = {
                let session = self.rtp_write_session.lock().await;
                session.clone()
            };
            if let Some(rtp_write_session) = session {
                return Ok(rtp_write_session.write_rtp(pkt).await?);
            }
        }

        Ok(0)
    }

    pub async fn write(&self, b: &Bytes) -> Result<usize> {
        {
            let session = {
                let session = self.rtp_write_session.lock().await;
                session.clone()
            };
            if let Some(rtp_write_session) = session {
                return Ok(rtp_write_session.write(b, true).await?);
            }
        }

        self.init(true).await?;

        {
            let session = {
                let session = self.rtp_write_session.lock().await;
                session.clone()
            };
            if let Some(rtp_write_session) = session {
                return Ok(rtp_write_session.write(b, true).await?);
            }
        }

        Ok(0)
    }
}

type IResult<T> = std::result::Result<T, interceptor::Error>;

#[async_trait]
impl RTCPReader for SrtpWriterFuture {
    async fn read(
        &self,
        buf: &mut [u8],
        a: &Attributes,
    ) -> IResult<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        let read = self.read(buf).await?;
        let pkt = rtcp::packet::unmarshal(&mut &buf[..read])?;

        Ok((pkt, a.clone()))
    }
}

#[async_trait]
impl RTPWriter for SrtpWriterFuture {
    async fn write(&self, pkt: &rtp::packet::Packet, _a: &Attributes) -> IResult<usize> {
        Ok(
            match self.seq_trans.seq_number(pkt.header.sequence_number) {
                Some(seq_num) => {
                    let mut new_pkt = pkt.clone();
                    new_pkt.header.sequence_number = seq_num;
                    self.write_rtp(&new_pkt).await?
                }
                None => self.write_rtp(pkt).await?,
            },
        )
    }
}
