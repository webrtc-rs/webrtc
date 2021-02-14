use crate::{
    errors::RTPError,
    packetizer::{Depacketizer, Payloader},
};

mod vp8_test;

const VP8_HEADER_SIZE: usize = 1;

pub struct VP8Payloader;

impl Payloader for VP8Payloader {
    fn payload(&self, mtu: u16, payload_data: &[u8]) -> Vec<Vec<u8>> {
        /*
         * https://tools.ietf.org/html/rfc7741#section-4.2
         *
         *       0 1 2 3 4 5 6 7
         *      +-+-+-+-+-+-+-+-+
         *      |X|R|N|S|R| PID | (REQUIRED)
         *      +-+-+-+-+-+-+-+-+
         * X:   |I|L|T|K| RSV   | (OPTIONAL)
         *      +-+-+-+-+-+-+-+-+
         * I:   |M| PictureID   | (OPTIONAL)
         *      +-+-+-+-+-+-+-+-+
         * L:   |   TL0PICIDX   | (OPTIONAL)
         *      +-+-+-+-+-+-+-+-+
         * T/K: |TID|Y| KEYIDX  | (OPTIONAL)
         *      +-+-+-+-+-+-+-+-+
         *  S: Start of VP8 partition.  SHOULD be set to 1 when the first payload
         *     octet of the RTP packet is the beginning of a new VP8 partition,
         *     and MUST NOT be 1 otherwise.  The S bit MUST be set to 1 for the
         *     first packet of each encoded frame.
         */

        let max_fragment_size = mtu as isize - VP8_HEADER_SIZE as isize;

        let mut payload_data_remaining = payload_data.len() as isize;

        let mut payload_data_index: usize = 0;
        let mut payloads = vec![];

        // Make sure the fragment/payload size is correct
        if max_fragment_size.min(payload_data_remaining) <= 0 {
            return payloads;
        }

        while payload_data_remaining > 0 {
            let current_fragment_size = max_fragment_size.min(payload_data_remaining) as usize;
            let mut out = vec![0u8; VP8_HEADER_SIZE + current_fragment_size];

            if payload_data_index == 0 {
                out[0] = 0x10;
            }

            out[VP8_HEADER_SIZE..VP8_HEADER_SIZE + current_fragment_size].copy_from_slice(
                &payload_data[payload_data_index..payload_data_index + current_fragment_size],
            );
            payloads.push(out);

            payload_data_remaining -= current_fragment_size as isize;
            payload_data_index += current_fragment_size;
        }

        payloads
    }
}

#[derive(Debug, Default)]
// VP8Packet represents the VP8 header that is stored in the payload of an RTP Packet
pub struct VP8Packet {
    // Required Header
    pub x: u8,   /* extended controlbits present */
    pub n: u8,   /* (non-reference frame)  when set to 1 this frame can be discarded */
    pub s: u8,   /* start of VP8 partition */
    pub pid: u8, /* partition index */

    // Optional Header
    pub i: u8, /* 1 if PictureID is present */
    pub l: u8, /* 1 if TL0PICIDX is present */
    pub t: u8, /* 1 if TID is present */
    pub k: u8, /* 1 if KEYIDX is present */

    pub picture_id: u16, /* 8 or 16 bits, picture ID */
    pub tl0_pic_idx: u8, /* 8 bits temporal level zero index */

    pub tid: u8,
    pub y: u8,
    pub key_idx: u8,

    pub payload: Vec<u8>,
}

impl Depacketizer for VP8Packet {
    // Unmarshal parses the passed byte slice and stores the result in the VP8Packet this method is called upon
    fn unmarshal(&mut self, payload: &mut [u8]) -> Result<Vec<u8>, RTPError> {
        let payload_len = payload.len();
        if payload_len < 4 {
            return Err(RTPError::ShortPacket);
        }

        let mut payload_index = 0;

        self.x = (payload[payload_index] & 0x80) >> 7;
        self.n = (payload[payload_index] & 0x20) >> 5;
        self.s = (payload[payload_index] & 0x10) >> 4;
        self.pid = payload[payload_index] & 0x07;

        payload_index += 1;

        if self.x == 1 {
            self.i = (payload[payload_index] & 0x80) >> 7;
            self.l = (payload[payload_index] & 0x40) >> 6;
            self.t = (payload[payload_index] & 0x20) >> 5;
            self.k = (payload[payload_index] & 0x10) >> 4;
            payload_index += 1;
        }

        // PID present?
        if self.i == 1 {
            // M == 1, PID is 16bit
            if payload[payload_index] & 0x80 > 0 {
                payload_index += 2;
            } else {
                payload_index += 1;
            }
        }

        if self.l == 1 {
            payload_index += 1;
        }

        if self.t == 1 || self.k == 1 {
            payload_index += 1;
        }

        if payload_index >= payload_len {
            return Err(RTPError::ShortPacket);
        }

        self.payload = payload[payload_index..].into();
        Ok(self.payload.to_owned())
    }
}

// VP8PartitionHeadChecker checks VP8 partition head
struct VP8PartitionHeadChecker;

impl VP8PartitionHeadChecker {
    pub fn is_partition_head(&mut self, mut packet: &mut [u8]) -> bool {
        let mut p = VP8Packet::default();
        if p.unmarshal(&mut packet).is_err() {
            return false;
        }

        p.s == 1
    }
}
