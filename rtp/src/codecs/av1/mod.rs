use bytes::{BufMut, Bytes, BytesMut};

use crate::codecs::av1::leb128::BytesMutExt;
use crate::codecs::av1::obu::{obu_has_extension, parse_obus, OBU_HAS_SIZE_BIT};
use crate::codecs::av1::packetizer::{
    get_aggregation_header, packetize, AGGREGATION_HEADER_SIZE, MAX_NUM_OBUS_TO_OMIT_SIZE,
};
use crate::packetizer::Payloader;

#[cfg(test)]
mod av1_test;
mod leb128;
mod obu;
mod packetizer;

#[derive(Default, Clone, Debug)]
pub struct Av1Payloader {}

impl Payloader for Av1Payloader {
    /// Based on <https://chromium.googlesource.com/external/webrtc/+/4e513346ec56c829b3a6010664998469fc237b35/modules/rtp_rtcp/source/rtp_packetizer_av1.cc>
    /// Reference: <https://aomediacodec.github.io/av1-rtp-spec/#45-payload-structure>
    fn payload(&mut self, mtu: usize, payload: &Bytes) -> crate::error::Result<Vec<Bytes>> {
        // 0                   1                   2                   3
        // 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |Z|Y|1 0|N|-|-|-|  OBU element 1 size (leb128)  |               |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+               |
        // |                                                               |
        // :                                                               :
        // :                      OBU element 1 data                       :
        // :                                                               :
        // |                                                               |
        // |                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |                               |                               |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+                               |
        // |                                                               |
        // :                                                               :
        // :                      OBU element 2 data                       :
        // :                                                               :
        // |                                                               |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

        // Parse the payload into series of OBUs.
        let obus = parse_obus(payload)?;

        // Packetize the OBUs, possibly aggregating multiple OBUs into a single packet,
        // or splitting a single OBU across multiple packets.
        let packets_metadata = packetize(&obus, mtu);

        let mut payloads = vec![];

        // Split the payload into RTP packets according to the packetization scheme.
        for packet_index in 0..packets_metadata.len() {
            let packet = &packets_metadata[packet_index];
            let mut obu_offset = packet.first_obu_offset;
            let aggregation_header = get_aggregation_header(&obus, &packets_metadata, packet_index);

            let mut out = BytesMut::with_capacity(AGGREGATION_HEADER_SIZE + packet.packet_size);
            out.put_u8(aggregation_header);

            // Store all OBU elements except the last one.
            for i in 0..packet.num_obu_elements - 1 {
                let obu = &obus[packet.first_obu_index + i];
                let fragment_size = obu.size - obu_offset;
                out.put_leb128(fragment_size as u32);
                if obu_offset == 0 {
                    out.put_u8(obu.header & !OBU_HAS_SIZE_BIT);
                }
                if obu_offset <= 1 && obu_has_extension(obu.header) {
                    out.put_u8(obu.extension_header);
                }
                let payload_offset = if obu_offset > obu.header_size() {
                    obu_offset - obu.header_size()
                } else {
                    0
                };
                let payload_size = obu.payload.len() - payload_offset;
                out.put_slice(
                    obu.payload
                        .slice(payload_offset..payload_offset + payload_size)
                        .as_ref(),
                );
                // All obus are stored from the beginning, except, may be, the first one.
                obu_offset = 0;
            }

            // Store the last OBU element.
            let last_obu = &obus[packet.first_obu_index + packet.num_obu_elements - 1];
            let mut fragment_size = packet.last_obu_size;
            if packet.num_obu_elements > MAX_NUM_OBUS_TO_OMIT_SIZE {
                out.put_leb128(fragment_size as u32);
            }
            if obu_offset == 0 && fragment_size > 0 {
                out.put_u8(last_obu.header & !OBU_HAS_SIZE_BIT);
                fragment_size -= 1;
            }
            if obu_offset <= 1 && obu_has_extension(last_obu.header) && fragment_size > 0 {
                out.put_u8(last_obu.extension_header);
                fragment_size -= 1;
            }
            let payload_offset = if obu_offset > last_obu.header_size() {
                obu_offset - last_obu.header_size()
            } else {
                0
            };
            out.put_slice(
                last_obu
                    .payload
                    .slice(payload_offset..payload_offset + fragment_size)
                    .as_ref(),
            );

            payloads.push(out.freeze());
        }

        Ok(payloads)
    }

    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync> {
        Box::new(self.clone())
    }
}
