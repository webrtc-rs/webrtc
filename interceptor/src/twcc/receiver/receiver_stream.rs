use super::*;

pub(super) struct ReceiverStream {
    parent_rtp_reader: Arc<dyn RTPReader + Send + Sync>,
    hdr_ext_id: u8,
    ssrc: u32,
    packet_chan_tx: mpsc::Sender<Packet>,
    start_time: SystemTime,
}

impl ReceiverStream {
    pub(super) fn new(
        parent_rtp_reader: Arc<dyn RTPReader + Send + Sync>,
        hdr_ext_id: u8,
        ssrc: u32,
        packet_chan_tx: mpsc::Sender<Packet>,
        start_time: SystemTime,
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
    async fn read(&self, buf: &mut [u8], attributes: &Attributes) -> Result<(usize, Attributes)> {
        let (n, attr) = self.parent_rtp_reader.read(buf, attributes).await?;

        let mut b = &buf[..n];
        let p = rtp::packet::Packet::unmarshal(&mut b)?;

        if let Some(mut ext) = p.header.get_extension(self.hdr_ext_id) {
            let tcc_ext = TransportCcExtension::unmarshal(&mut ext)?;

            let _ = self
                .packet_chan_tx
                .send(Packet {
                    hdr: p.header,
                    sequence_number: tcc_ext.transport_sequence,
                    arrival_time: SystemTime::now()
                        .duration_since(self.start_time)
                        .unwrap_or_else(|_| Duration::from_secs(0))
                        .as_micros() as i64,
                    ssrc: self.ssrc,
                })
                .await;
        }

        Ok((n, attr))
    }
}
