#[cfg(test)]
mod vp8_test;

use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::error::{Error, Result};
use crate::packetizer::{Depacketizer, Payloader};

pub const VP8_HEADER_SIZE: usize = 1;

/// Vp8Payloader payloads VP8 packets
#[derive(Default, Debug, Copy, Clone)]
pub struct Vp8Payloader {
    pub enable_picture_id: bool,
    picture_id: u16,
}

impl Payloader for Vp8Payloader {
    /// Payload fragments a VP8 packet across one or more byte arrays
    fn payload(&mut self, mtu: usize, payload: &Bytes) -> Result<Vec<Bytes>> {
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
         * L:   |   tl0picidx   | (OPTIONAL)
         *      +-+-+-+-+-+-+-+-+
         * T/K: |tid|Y| KEYIDX  | (OPTIONAL)
         *      +-+-+-+-+-+-+-+-+
         *  S: Start of VP8 partition.  SHOULD be set to 1 when the first payload
         *     octet of the RTP packet is the beginning of a new VP8 partition,
         *     and MUST NOT be 1 otherwise.  The S bit MUST be set to 1 for the
         *     first packet of each encoded frame.
         */
        let using_header_size = if self.enable_picture_id {
            if self.picture_id == 0 || self.picture_id < 128 {
                VP8_HEADER_SIZE + 2
            } else {
                VP8_HEADER_SIZE + 3
            }
        } else {
            VP8_HEADER_SIZE
        };

        let max_fragment_size = mtu as isize - using_header_size as isize;
        let mut payload_data_remaining = payload.len() as isize;
        let mut payload_data_index: usize = 0;
        let mut payloads = vec![];

        // Make sure the fragment/payload size is correct
        if std::cmp::min(max_fragment_size, payload_data_remaining) <= 0 {
            return Ok(payloads);
        }

        let mut first = true;
        while payload_data_remaining > 0 {
            let current_fragment_size =
                std::cmp::min(max_fragment_size, payload_data_remaining) as usize;
            let mut out = BytesMut::with_capacity(using_header_size + current_fragment_size);
            let mut buf = [0u8; 4];
            if first {
                buf[0] = 0x10;
                first = false;
            }

            if self.enable_picture_id {
                if using_header_size == VP8_HEADER_SIZE + 2 {
                    buf[0] |= 0x80;
                    buf[1] |= 0x80;
                    buf[2] |= (self.picture_id & 0x7F) as u8;
                } else if using_header_size == VP8_HEADER_SIZE + 3 {
                    buf[0] |= 0x80;
                    buf[1] |= 0x80;
                    buf[2] |= 0x80 | ((self.picture_id >> 8) & 0x7F) as u8;
                    buf[3] |= (self.picture_id & 0xFF) as u8;
                }
            }

            out.put(&buf[..using_header_size]);

            out.put(
                &*payload.slice(payload_data_index..payload_data_index + current_fragment_size),
            );
            payloads.push(out.freeze());

            payload_data_remaining -= current_fragment_size as isize;
            payload_data_index += current_fragment_size;
        }

        self.picture_id += 1;
        self.picture_id &= 0x7FFF;

        Ok(payloads)
    }

    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync> {
        Box::new(*self)
    }
}

/// Vp8Packet represents the VP8 header that is stored in the payload of an RTP Packet
#[derive(PartialEq, Eq, Debug, Default, Clone)]
pub struct Vp8Packet {
    /// Required Header
    /// extended controlbits present
    pub x: u8,
    /// when set to 1 this frame can be discarded
    pub n: u8,
    /// start of VP8 partition
    pub s: u8,
    /// partition index
    pub pid: u8,

    /// Extended control bits
    /// 1 if PictureID is present
    pub i: u8,
    /// 1 if tl0picidx is present
    pub l: u8,
    /// 1 if tid is present
    pub t: u8,
    /// 1 if KEYIDX is present
    pub k: u8,

    /// Optional extension
    /// 8 or 16 bits, picture ID
    pub picture_id: u16,
    /// 8 bits temporal level zero index
    pub tl0_pic_idx: u8,
    /// 2 bits temporal layer index
    pub tid: u8,
    /// 1 bit layer sync bit
    pub y: u8,
    /// 5 bits temporal key frame index
    pub key_idx: u8,
}

impl Depacketizer for Vp8Packet {
    /// depacketize parses the passed byte slice and stores the result in the VP8Packet this method is called upon
    fn depacketize(&mut self, packet: &Bytes) -> Result<Bytes> {
        let payload_len = packet.len();
        if payload_len < 4 {
            return Err(Error::ErrShortPacket);
        }
        //    0 1 2 3 4 5 6 7                      0 1 2 3 4 5 6 7
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        //    |X|R|N|S|R| PID | (REQUIRED)        |X|R|N|S|R| PID | (REQUIRED)
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        // X: |I|L|T|K| RSV   | (OPTIONAL)   X:   |I|L|T|K| RSV   | (OPTIONAL)
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        // I: |M| PictureID   | (OPTIONAL)   I:   |M| PictureID   | (OPTIONAL)
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        // L: |   tl0picidx   | (OPTIONAL)        |   PictureID   |
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        //T/K:|tid|Y| KEYIDX  | (OPTIONAL)   L:   |   tl0picidx   | (OPTIONAL)
        //    +-+-+-+-+-+-+-+-+                   +-+-+-+-+-+-+-+-+
        //T/K:|tid|Y| KEYIDX  | (OPTIONAL)
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

        if payload_index >= payload_len {
            return Err(Error::ErrShortPacket);
        }

        if self.l == 1 {
            self.tl0_pic_idx = reader.get_u8();
            payload_index += 1;
        }

        if payload_index >= payload_len {
            return Err(Error::ErrShortPacket);
        }

        if self.t == 1 || self.k == 1 {
            let b = reader.get_u8();
            if self.t == 1 {
                self.tid = b >> 6;
                self.y = (b >> 5) & 0x1;
            }
            if self.k == 1 {
                self.key_idx = b & 0x1F;
            }
            payload_index += 1;
        }

        if payload_index >= packet.len() {
            return Err(Error::ErrShortPacket);
        }

        Ok(packet.slice(payload_index..))
    }

    /// is_partition_head checks whether if this is a head of the VP8 partition
    fn is_partition_head(&self, payload: &Bytes) -> bool {
        if payload.is_empty() {
            false
        } else {
            (payload[0] & 0x10) != 0
        }
    }

    fn is_partition_tail(&self, marker: bool, _payload: &Bytes) -> bool {
        marker
    }
}
