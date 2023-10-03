use super::*;

pub(super) struct ReceiverStream {
    parent_rtp_reader: Arc<dyn RTPReader + Send + Sync>,
    hdr_ext_id: u8,
    ssrc: u32,
    packet_chan_tx: mpsc::Sender<Packet>,
    // we use tokio's Instant because it makes testing easier via `tokio::time::advance`.
    start_time: tokio::time::Instant,
}

impl ReceiverStream {
    pub(super) fn new(
        parent_rtp_reader: Arc<dyn RTPReader + Send + Sync>,
        hdr_ext_id: u8,
        ssrc: u32,
        packet_chan_tx: mpsc::Sender<Packet>,
        start_time: tokio::time::Instant,
    ) -> Self {
        ReceiverStream {
            parent_rtp_reader,
            hdr_ext_id,
            ssrc,
            packet_chan_tx,
            start_time,
        }
    }
}

#[async_trait]
impl RTPReader for ReceiverStream {
    /// read a rtp packet
    async fn read(
        &self,
        buf: &mut [u8],
        attributes: &Attributes,
    ) -> Result<(rtp::packet::Packet, Attributes)> {
        let (pkt, attr) = self.parent_rtp_reader.read(buf, attributes).await?;

        if let Some(mut ext) = pkt.header.get_extension(self.hdr_ext_id) {
            let tcc_ext = TransportCcExtension::unmarshal(&mut ext)?;

            let _ = self
                .packet_chan_tx
                .send(Packet {
                    hdr: pkt.header.clone(),
                    sequence_number: tcc_ext.transport_sequence,
                    arrival_time: (tokio::time::Instant::now() - self.start_time).as_micros()
                        as i64,
                    ssrc: self.ssrc,
                })
                .await;
        }

        Ok((pkt, attr))
    }
}
