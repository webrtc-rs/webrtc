#[cfg(test)]
mod vp8_test;

use crate::{
    error::Error,
    packetizer::{Depacketizer, Payloader},
};

use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};

pub const VP8_HEADER_SIZE: isize = 1;

/// Vp8Payloader payloads VP8 packets
#[derive(Debug, Copy, Clone)]
pub struct Vp8Payloader;

impl Payloader for Vp8Payloader {
    /// Payload fragments a VP8 packet across one or more byte arrays
    fn payload(&self, mtu: usize, payload: &Bytes) -> Result<Vec<Bytes>> {
        if payload.is_empty() || mtu == 0 {
            return Ok(vec![]);
        }

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

        let max_fragment_size = mtu as isize - VP8_HEADER_SIZE;
        let mut payload_data_remaining = payload.len() as isize;
        let mut payload_data_index: usize = 0;
        let mut payloads = vec![];

        // Make sure the fragment/payload size is correct
        if std::cmp::min(max_fragment_size, payload_data_remaining) <= 0 {
            return Ok(payloads);
        }

        while payload_data_remaining > 0 {
            let current_fragment_size =
                std::cmp::min(max_fragment_size, payload_data_remaining) as usize;
            let mut out = BytesMut::with_capacity(VP8_HEADER_SIZE as usize + current_fragment_size);
            if payload_data_remaining == payload.len() as isize {
                out.put_u8(0x10);
            }

            out.put(
                &*payload.slice(payload_data_index..payload_data_index + current_fragment_size),
            );
            payloads.push(out.freeze());

            payload_data_remaining -= current_fragment_size as isize;
            payload_data_index += current_fragment_size;
        }

        Ok(payloads)
    }

    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync> {
        Box::new(*self)
    }
}

/// Vp8Packet represents the VP8 header that is stored in the payload of an RTP Packet
#[derive(PartialEq, Debug, Default, Clone)]
pub struct Vp8Packet {
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

    pub payload: Bytes,
}

impl Depacketizer for Vp8Packet {
    /// depacketize parses the passed byte slice and stores the result in the VP8Packet this method is called upon
    fn depacketize(&mut self, packet: &Bytes) -> Result<()> {
        if packet.len() < 4 {
            return Err(Error::ErrShortPacket.into());
        }
        //    0 1 2 3 4 5 6 7                      0 1 2 3 4 5 6 7
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        //    |X|R|N|S|R| PID | (REQUIRED)        |X|R|N|S|R| PID | (REQUIRED)
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        // X: |I|L|T|K| RSV   | (OPTIONAL)   X:   |I|L|T|K| RSV   | (OPTIONAL)
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        // I: |M| PictureID   | (OPTIONAL)   I:   |M| PictureID   | (OPTIONAL)
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        // L: |   TL0PICIDX   | (OPTIONAL)        |   PictureID   |
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        //T/K:|TID|Y| KEYIDX  | (OPTIONAL)   L:   |   TL0PICIDX   | (OPTIONAL)
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        //T/K:|TID|Y| KEYIDX  | (OPTIONAL)
        //    +-+-+-+-+-+-+-+-+

        let reader = &mut packet.clone();
        let mut payload_index = 0;

        let mut b = reader.get_u8();
        payload_index += 1;

        self.x = (b & 0x80) >> 7;
        self.n = (b & 0x20) >> 5;
        self.s = (b & 0x10) >> 4;
        self.pid = b & 0x07;

        if self.x == 1 {
            b = reader.get_u8();
            payload_index += 1;
            self.i = (b & 0x80) >> 7;
            self.l = (b & 0x40) >> 6;
            self.t = (b & 0x20) >> 5;
            self.k = (b & 0x10) >> 4;
        } else {
            self.i = 0;
            self.l = 0;
            self.t = 0;
            self.k = 0;
        }

        if self.i == 1 {
            b = reader.get_u8();
            payload_index += 1;
            // PID present?
            if b & 0x80 > 0 {
                // M == 1, PID is 16bit
                self.picture_id = (((b & 0x7f) as u16) << 8) | (reader.get_u8() as u16);
                payload_index += 1;
            } else {
                self.picture_id = b as u16;
            }
        }

        if self.l == 1 {
            self.tl0_pic_idx = reader.get_u8();
            payload_index += 1;
        }

        if self.t == 1 || self.k == 1 {
            b = reader.get_u8();
            payload_index += 1;
            self.tid = (b & 0b11000000) >> 6;
            self.y = (b & 0b00100000) >> 5;
            self.key_idx = b & 0b00011111;
        }

        if payload_index >= packet.len() {
            return Err(Error::ErrShortPacket.into());
        }

        self.payload = packet.slice(payload_index..);

        Ok(())
    }
}
