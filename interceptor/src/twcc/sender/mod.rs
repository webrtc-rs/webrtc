mod sender_stream;
#[cfg(test)]
mod sender_test;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use portable_atomic::AtomicU32;
use rtp::extension::transport_cc_extension::TransportCcExtension;
use sender_stream::SenderStream;
use tokio::sync::Mutex;
use util::Marshal;

use crate::{Attributes, RTPWriter, *};

pub(crate) const TRANSPORT_CC_URI: &str =
    "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01";

/// HeaderExtensionBuilder is a InterceptorBuilder for a HeaderExtension Interceptor
#[derive(Default)]
pub struct SenderBuilder {
    init_sequence_nr: u32,
}

impl SenderBuilder {
    /// with_init_sequence_nr sets the init sequence number of the interceptor.
    pub fn with_init_sequence_nr(mut self, init_sequence_nr: u32) -> SenderBuilder {
        self.init_sequence_nr = init_sequence_nr;
        self
    }
}

impl InterceptorBuilder for SenderBuilder {
    /// build constructs a new SenderInterceptor
    fn build(&self, _id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        Ok(Arc::new(Sender {
            next_sequence_nr: Arc::new(AtomicU32::new(self.init_sequence_nr)),
            streams: Mutex::new(HashMap::new()),
        }))
    }
}

/// Sender adds transport wide sequence numbers as header extension to each RTP packet
pub struct Sender {
    next_sequence_nr: Arc<AtomicU32>,
    streams: Mutex<HashMap<u32, Arc<SenderStream>>>,
}

impl Sender {
    /// builder returns a new SenderBuilder.
    pub fn builder() -> SenderBuilder {
        SenderBuilder::default()
    }
}

#[async_trait]
impl Interceptor for Sender {
    /// bind_rtcp_reader lets you modify any incoming RTCP packets. It is called once per sender/receiver, however this might
    /// change in the future. The returned method will be called once per packet batch.
    async fn bind_rtcp_reader(
        &self,
        reader: Arc<dyn RTCPReader + Send + Sync>,
    ) -> Arc<dyn RTCPReader + Send + Sync> {
        reader
    }

    /// bind_rtcp_writer lets you modify any outgoing RTCP packets. It is called once per PeerConnection. The returned method
    /// will be called once per packet batch.
    async fn bind_rtcp_writer(
        &self,
        writer: Arc<dyn RTCPWriter + Send + Sync>,
    ) -> Arc<dyn RTCPWriter + Send + Sync> {
        writer
    }

    /// bind_local_stream returns a writer that adds a rtp TransportCCExtension
    /// header with increasing sequence numbers to each outgoing packet.
    async fn bind_local_stream(
        &self,
        info: &StreamInfo,
        writer: Arc<dyn RTPWriter + Send + Sync>,
    ) -> Arc<dyn RTPWriter + Send + Sync> {
        let mut hdr_ext_id = 0u8;
        for e in &info.rtp_header_extensions {
            if e.uri == TRANSPORT_CC_URI {
                hdr_ext_id = e.id as u8;
                break;
            }
        }
        if hdr_ext_id == 0 {
            // Don't add header extension if ID is 0, because 0 is an invalid extension ID
            return writer;
        }

        let stream = Arc::new(SenderStream::new(
            writer,
            Arc::clone(&self.next_sequence_nr),
            hdr_ext_id,
        ));

        {
            let mut streams = self.streams.lock().await;
            streams.insert(info.ssrc, Arc::clone(&stream));
        }

        stream
    }

    /// unbind_local_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_local_stream(&self, info: &StreamInfo) {
        let mut streams = self.streams.lock().await;
        streams.remove(&info.ssrc);
    }

    /// bind_remote_stream lets you modify any incoming RTP packets. It is called once for per RemoteStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_remote_stream(
        &self,
        _info: &StreamInfo,
        reader: Arc<dyn RTPReader + Send + Sync>,
    ) -> Arc<dyn RTPReader + Send + Sync> {
        reader
    }

    /// unbind_remote_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_remote_stream(&self, _info: &StreamInfo) {}

    /// close closes the Interceptor, cleaning up any data if necessary.
    async fn close(&self) -> Result<()> {
        Ok(())
    }
}
