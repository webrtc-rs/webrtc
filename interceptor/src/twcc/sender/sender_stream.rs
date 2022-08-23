use super::*;

pub(super) struct SenderStream {
    next_rtp_writer: Arc<dyn RTPWriter + Send + Sync>,
    next_sequence_nr: Arc<AtomicU32>,
    hdr_ext_id: u8,
}

impl SenderStream {
    pub(super) fn new(
        next_rtp_writer: Arc<dyn RTPWriter + Send + Sync>,
        next_sequence_nr: Arc<AtomicU32>,
        hdr_ext_id: u8,
    ) -> Self {
        SenderStream {
            next_rtp_writer,
            next_sequence_nr,
            hdr_ext_id,
        }
    }
}

/// RTPWriter is used by Interceptor.bind_local_stream.
#[async_trait]
impl RTPWriter for SenderStream {
    /// write a rtp packet
    async fn write(&self, pkt: &rtp::packet::Packet, a: &Attributes) -> Result<usize> {
        let sequence_number = self.next_sequence_nr.fetch_add(1, Ordering::SeqCst);

        let tcc_ext = TransportCcExtension {
            transport_sequence: sequence_number as u16,
        };
        let tcc_payload = tcc_ext.marshal()?;

        let mut pkt = pkt.clone();
        pkt.header.set_extension(self.hdr_ext_id, tcc_payload)?;

        self.next_rtp_writer.write(&pkt, a).await
    }
}
