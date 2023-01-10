use crate::dtls_transport::RTCDtlsTransport;
use crate::error::{Error, Result};
use crate::rtp_transceiver::rtp_sender::RTPSenderInternal;
use crate::rtp_transceiver::SSRC;

use srtp::session::Session;
use srtp::stream::Stream;

use async_trait::async_trait;
use bytes::Bytes;
use interceptor::{Attributes, RTCPReader, RTPWriter};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

pub struct SequenceTransformer {
    offset: AtomicU16,
    last_sq: AtomicU16,
    reset_needed: AtomicBool,
}

impl SequenceTransformer {
    pub fn new() -> Self {
        Self {
            offset: AtomicU16::new(0),
            last_sq: AtomicU16::new(rand::random()),
            reset_needed: AtomicBool::new(false),
        }
    }

    pub fn reset_offset(&self) {
        self.reset_needed.store(true, Ordering::SeqCst);
    }

    fn seq_number(&self, raw_sn: u16) -> u16 {
        let offset = self
            .reset_needed
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
            .then(|| {
                let offset = self
                    .last_sq
                    .load(Ordering::SeqCst)
                    .overflowing_sub(raw_sn)
                    .0;
                self.offset.store(offset, Ordering::SeqCst);

                offset
            })
            .unwrap_or(self.offset.load(Ordering::SeqCst));
        let next = raw_sn.overflowing_add(offset).0;
        self.last_sq.store(next, Ordering::SeqCst);

        next
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
    pub(crate) seq_trans: Option<Arc<SequenceTransformer>>,
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
    async fn read(&self, buf: &mut [u8], a: &Attributes) -> IResult<(usize, Attributes)> {
        Ok((self.read(buf).await?, a.clone()))
    }
}

#[async_trait]
impl RTPWriter for SrtpWriterFuture {
    async fn write(&self, pkt: &rtp::packet::Packet, _a: &Attributes) -> IResult<usize> {
        let res = if let Some(st) = &self.seq_trans {
            let mut pkt = pkt.clone();
            pkt.header.sequence_number = st.seq_number(pkt.header.sequence_number);

            self.write_rtp(&pkt).await
        } else {
            self.write_rtp(pkt).await
        };

        Ok(res?)
    }
}
