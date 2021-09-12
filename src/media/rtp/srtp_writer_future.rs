use crate::error::Error;
use crate::media::dtls_transport::DTLSTransport;
use crate::media::rtp::rtp_sender::RTPSenderInternal;
use crate::media::rtp::SSRC;

use srtp::session::Session;
use srtp::stream::Stream;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use interceptor::{Attributes, RTCPReader};
use std::sync::Arc;
use tokio::sync::Mutex;

/// SrtpWriterFuture blocks Read/Write calls until
/// the SRTP Session is available
pub(crate) struct SrtpWriterFuture {
    pub(crate) ssrc: SSRC,
    pub(crate) rtp_sender: Arc<Mutex<RTPSenderInternal>>,
    pub(crate) rtp_transport: Arc<DTLSTransport>,
    pub(crate) rtcp_read_stream: Mutex<Option<Arc<Stream>>>, // atomic.Value // *
    pub(crate) rtp_write_session: Mutex<Option<Arc<Session>>>, // atomic.Value // *
}

impl SrtpWriterFuture {
    async fn init(&self, return_when_no_srtp: bool) -> Result<()> {
        {
            let mut rx = self.rtp_transport.srtp_ready_rx.lock().await;
            if let Some(srtp_ready_rx) = &mut *rx {
                let mut rtp_sender = self.rtp_sender.lock().await;
                if return_when_no_srtp {
                    tokio::select! {
                        _ = rtp_sender.stop_called_rx.recv()=> return Err(Error::ErrClosedPipe.into()),
                        _ = srtp_ready_rx.recv() =>{}
                        else => {  //TODO: How to implement default?
                            return Ok(());
                        }
                    }
                } else {
                    tokio::select! {
                        _ = rtp_sender.stop_called_rx.recv()=> return Err(Error::ErrClosedPipe.into()),
                        _ = srtp_ready_rx.recv() =>{}
                    }
                }
            }
        }

        if let Some(srtcp_session) = self.rtp_transport.get_srtcp_session().await {
            //TODO: use srtcp_session.open(self.ssrc).await?
            let rtcp_read_stream = Arc::new(srtcp_session.listen(self.ssrc).await?);
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
        let stream = self.rtcp_read_stream.lock().await;
        if let Some(rtcp_read_stream) = &*stream {
            rtcp_read_stream.close().await
        } else {
            Ok(())
        }
    }

    pub async fn read(&self, b: &mut [u8]) -> Result<usize> {
        {
            let stream = self.rtcp_read_stream.lock().await;
            if let Some(rtcp_read_stream) = &*stream {
                return rtcp_read_stream.read(b).await;
            }
        }

        self.init(false).await?;

        {
            let stream = self.rtcp_read_stream.lock().await;
            if let Some(rtcp_read_stream) = &*stream {
                rtcp_read_stream.read(b).await
            } else {
                Err(Error::ErrDtlsTransportNotStarted.into())
            }
        }
    }

    pub async fn write_rtp(&self, packet: &rtp::packet::Packet) -> Result<usize> {
        {
            let session = self.rtp_write_session.lock().await;
            if let Some(rtp_write_session) = &*session {
                return rtp_write_session.write_rtp(packet).await;
            }
        }

        self.init(true).await?;

        {
            let session = self.rtp_write_session.lock().await;
            if let Some(rtp_write_session) = &*session {
                rtp_write_session.write_rtp(packet).await
            } else {
                Err(Error::ErrDtlsTransportNotStarted.into())
            }
        }
    }

    pub async fn write(&self, b: &Bytes) -> Result<usize> {
        {
            let session = self.rtp_write_session.lock().await;
            if let Some(rtp_write_session) = &*session {
                return rtp_write_session.write(b, true).await;
            }
        }

        self.init(true).await?;

        {
            let session = self.rtp_write_session.lock().await;
            if let Some(rtp_write_session) = &*session {
                rtp_write_session.write(b, true).await
            } else {
                Err(Error::ErrDtlsTransportNotStarted.into())
            }
        }
    }
}

#[async_trait]
impl RTCPReader for SrtpWriterFuture {
    async fn read(&self, buf: &mut [u8], a: &Attributes) -> Result<(usize, Attributes)> {
        Ok((self.read(buf).await?, a.clone()))
    }
}
