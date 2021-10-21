use crate::*;
use crate::{Attributes, RTPWriter};

use rtp::extension::transport_cc_extension::TransportCcExtension;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use util::Marshal;

pub(crate) const TRANSPORT_CC_URI: &str =
    "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01";

/// HeaderExtensionBuilder is a InterceptorBuilder for a HeaderExtension Interceptor
#[derive(Default)]
pub struct HeaderExtensionBuilder {
    init_sequence_nr: u32,
}

impl HeaderExtensionBuilder {
    /// with_init_sequence_nr sets the init sequence number of the interceptor.
    pub fn with_init_sequence_nr(mut self, init_sequence_nr: u32) -> HeaderExtensionBuilder {
        self.init_sequence_nr = init_sequence_nr;
        self
    }
}

impl InterceptorBuilder for HeaderExtensionBuilder {
    /// build constructs a new HeaderExtensionInterceptor
    fn build(&self, _id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        Ok(Arc::new(HeaderExtension {
            internal: Arc::new(HeaderExtensionInternal {
                next_sequence_nr: AtomicU32::new(self.init_sequence_nr),
                ..Default::default()
            }),
        }))
    }
}

#[derive(Default)]
struct HeaderExtensionInternal {
    next_sequence_nr: AtomicU32,
    hdr_ext_id: AtomicU8,
    next_rtp_writer: Mutex<Option<Arc<dyn RTPWriter + Send + Sync>>>,
}

/// RTPWriter is used by Interceptor.bind_local_stream.
#[async_trait]
impl RTPWriter for HeaderExtensionInternal {
    /// write a rtp packet
    async fn write(&self, pkt: &rtp::packet::Packet, a: &Attributes) -> Result<usize> {
        let sequence_number = self.next_sequence_nr.fetch_add(1, Ordering::SeqCst);

        let tcc_ext = TransportCcExtension {
            transport_sequence: sequence_number as u16,
        };
        let tcc_payload = tcc_ext.marshal()?;

        let mut pkt = pkt.clone();
        pkt.header
            .set_extension(self.hdr_ext_id.load(Ordering::SeqCst), tcc_payload)?;

        let next_rtp_writer = {
            let next_rtp_writer = self.next_rtp_writer.lock().await;
            next_rtp_writer.clone()
        };

        if let Some(next_rtp_writer) = next_rtp_writer {
            next_rtp_writer.write(&pkt, a).await
        } else {
            Err(Error::ErrInvalidNextRtpWriter)
        }
    }
}

/// HeaderExtension adds transport wide sequence numbers as header extension to each RTP packet
pub struct HeaderExtension {
    internal: Arc<HeaderExtensionInternal>,
}

impl HeaderExtension {
    /// builder returns a new HeaderExtensionBuilder.
    pub fn builder() -> HeaderExtensionBuilder {
        HeaderExtensionBuilder::default()
    }
}

#[async_trait]
impl Interceptor for HeaderExtension {
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

        {
            self.internal.hdr_ext_id.store(hdr_ext_id, Ordering::SeqCst);
            let mut next_rtp_writer = self.internal.next_rtp_writer.lock().await;
            *next_rtp_writer = Some(writer);
        }

        Arc::clone(&self.internal) as Arc<dyn RTPWriter + Send + Sync>
    }

    /// unbind_local_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_local_stream(&self, _info: &StreamInfo) {}

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
