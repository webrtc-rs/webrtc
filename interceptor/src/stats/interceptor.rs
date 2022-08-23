use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use super::{RTPStats, RTPStatsReader};
use async_trait::async_trait;
use util::sync::Mutex;
use util::{MarshalSize, Unmarshal};

use crate::error::Result;
use crate::stream_info::StreamInfo;
use crate::{Attributes, Interceptor, RTCPReader, RTCPWriter, RTPReader, RTPWriter};

#[derive(Debug)]
pub struct StatsInterceptor {
    recv_streams: Mutex<HashMap<u32, Arc<RTPReadRecorder>>>,
    send_streams: Mutex<HashMap<u32, Arc<RTPWriteRecorder>>>,
    id: String,
}

impl StatsInterceptor {
    pub fn new(id: String) -> Self {
        Self {
            id,
            recv_streams: Default::default(),
            send_streams: Default::default(),
        }
    }

    pub fn recv_stats_reader(&self, ssrc: u32) -> Option<RTPStatsReader> {
        self.recv_stats_readers([ssrc].iter().copied())
            .into_iter()
            .next()
    }

    pub fn recv_stats_readers(&self, ssrcs: impl Iterator<Item = u32>) -> Vec<RTPStatsReader> {
        let lock = self.recv_streams.lock();

        ssrcs
            .filter_map(|ssrc| lock.get(&ssrc).map(|r| r.reader()))
            .collect()
    }

    pub fn send_stats_reader(&self, ssrc: u32) -> Option<RTPStatsReader> {
        self.send_stats_readers([ssrc].iter().copied())
            .into_iter()
            .next()
    }

    pub fn send_stats_readers(&self, ssrcs: impl Iterator<Item = u32>) -> Vec<RTPStatsReader> {
        let lock = self.send_streams.lock();

        ssrcs
            .filter_map(|ssrc| lock.get(&ssrc).map(|r| r.reader()))
            .collect()
    }
}

#[async_trait]
impl Interceptor for StatsInterceptor {
    /// bind_remote_stream lets you modify any incoming RTP packets. It is called once for per RemoteStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_remote_stream(
        &self,
        info: &StreamInfo,
        reader: Arc<dyn RTPReader + Send + Sync>,
    ) -> Arc<dyn RTPReader + Send + Sync> {
        let mut lock = self.recv_streams.lock();

        let e = lock
            .entry(info.ssrc)
            .or_insert_with(|| Arc::new(RTPReadRecorder::new(reader)));

        e.clone()
    }

    /// unbind_remote_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_remote_stream(&self, info: &StreamInfo) {
        let mut lock = self.recv_streams.lock();

        lock.remove(&info.ssrc);
    }

    /// bind_local_stream lets you modify any outgoing RTP packets. It is called once for per LocalStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_local_stream(
        &self,
        info: &StreamInfo,
        writer: Arc<dyn RTPWriter + Send + Sync>,
    ) -> Arc<dyn RTPWriter + Send + Sync> {
        let mut lock = self.send_streams.lock();

        let e = lock
            .entry(info.ssrc)
            .or_insert_with(|| Arc::new(RTPWriteRecorder::new(writer)));

        e.clone()
    }

    /// unbind_local_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_local_stream(&self, info: &StreamInfo) {
        let mut lock = self.send_streams.lock();

        lock.remove(&info.ssrc);
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }

    /// bind_rtcp_writer lets you modify any outgoing RTCP packets. It is called once per PeerConnection. The returned method
    /// will be called once per packet batch.
    async fn bind_rtcp_writer(
        &self,
        writer: Arc<dyn RTCPWriter + Send + Sync>,
    ) -> Arc<dyn RTCPWriter + Send + Sync> {
        // NOP
        writer
    }

    /// bind_rtcp_reader lets you modify any incoming RTCP packets. It is called once per sender/receiver, however this might
    /// change in the future. The returned method will be called once per packet batch.
    async fn bind_rtcp_reader(
        &self,
        reader: Arc<dyn RTCPReader + Send + Sync>,
    ) -> Arc<dyn RTCPReader + Send + Sync> {
        reader
    }
}

pub struct RTPReadRecorder {
    rtp_reader: Arc<dyn RTPReader + Send + Sync>,
    stats: RTPStats,
}

impl RTPReadRecorder {
    fn new(rtp_reader: Arc<dyn RTPReader + Send + Sync>) -> Self {
        Self {
            rtp_reader,
            stats: Default::default(),
        }
    }

    fn reader(&self) -> RTPStatsReader {
        self.stats.reader()
    }
}

#[async_trait]
impl RTPReader for RTPReadRecorder {
    async fn read(&self, buf: &mut [u8], attributes: &Attributes) -> Result<(usize, Attributes)> {
        let (bytes_read, attributes) = self.rtp_reader.read(buf, attributes).await?;
        // TODO: This parsing happens redundantly in several interceptors, would be good if we
        // could not do this.
        let mut b = &buf[..bytes_read];
        let packet = rtp::packet::Packet::unmarshal(&mut b)?;

        self.stats.update(
            (bytes_read - packet.payload.len()) as u64,
            packet.payload.len() as u64,
            1,
        );

        Ok((bytes_read, attributes))
    }
}

impl fmt::Debug for RTPReadRecorder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RTPReadRecorder")
            .field("stats", &self.stats)
            .finish()
    }
}

pub struct RTPWriteRecorder {
    rtp_writer: Arc<dyn RTPWriter + Send + Sync>,
    stats: RTPStats,
}

impl RTPWriteRecorder {
    fn new(rtp_writer: Arc<dyn RTPWriter + Send + Sync>) -> Self {
        Self {
            rtp_writer,
            stats: Default::default(),
        }
    }

    fn reader(&self) -> RTPStatsReader {
        self.stats.reader()
    }
}

#[async_trait]
impl RTPWriter for RTPWriteRecorder {
    /// write a rtp packet
    async fn write(&self, pkt: &rtp::packet::Packet, attributes: &Attributes) -> Result<usize> {
        let n = self.rtp_writer.write(pkt, attributes).await?;

        self.stats.update(
            pkt.header.marshal_size() as u64,
            pkt.payload.len() as u64,
            1,
        );

        Ok(n)
    }
}

impl fmt::Debug for RTPWriteRecorder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RTPWriteRecorder")
            .field("stats", &self.stats)
            .finish()
    }
}

#[cfg(test)]
mod test {
    use bytes::Bytes;

    use std::sync::Arc;

    use crate::error::Result;
    use crate::mock::mock_stream::MockStream;
    use crate::stream_info::StreamInfo;

    use super::StatsInterceptor;

    #[tokio::test]
    async fn test_stats_interceptor() -> Result<()> {
        let icpr: Arc<_> = Arc::new(StatsInterceptor::new("Hello".to_owned()));

        let recv_stream = MockStream::new(
            &StreamInfo {
                ssrc: 123456,
                ..Default::default()
            },
            icpr.clone(),
        )
        .await;

        let send_stream = MockStream::new(
            &StreamInfo {
                ssrc: 234567,
                ..Default::default()
            },
            icpr.clone(),
        )
        .await;

        let recv_reader = icpr
            .recv_stats_reader(123456)
            .expect("After binding recv_stats_reader should return Some");

        let send_reader = icpr
            .send_stats_reader(234567)
            .expect("After binding send_stats_reader should return Some");

        let _ = recv_stream
            .receive_rtp(rtp::packet::Packet {
                header: rtp::header::Header {
                    ..Default::default()
                },
                payload: Bytes::from_static(b"\xde\xad\xbe\xef"),
            })
            .await;

        let _ = recv_stream
            .read_rtp()
            .await
            .expect("After calling receive_rtp read_rtp should return Some")?;

        assert_eq!(recv_reader.packets(), 1);
        assert_eq!(recv_reader.header_bytes(), 12);
        assert_eq!(recv_reader.payload_bytes(), 4);

        let _ = send_stream
            .write_rtp(&rtp::packet::Packet {
                header: rtp::header::Header {
                    ..Default::default()
                },
                payload: Bytes::from_static(b"\xde\xad\xbe\xef\xde\xad\xbe\xef"),
            })
            .await;

        let _ = send_stream
            .write_rtp(&rtp::packet::Packet {
                header: rtp::header::Header {
                    ..Default::default()
                },
                payload: Bytes::from_static(&[0x13, 0x37]),
            })
            .await;

        assert_eq!(send_reader.packets(), 2);
        assert_eq!(send_reader.header_bytes(), 24);
        assert_eq!(send_reader.payload_bytes(), 10);

        Ok(())
    }
}
