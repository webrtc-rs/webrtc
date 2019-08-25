use crate::packetizer::{Depacketizer, Payloader};

use std::io::Read;

use byteorder::ReadBytesExt;
use utils::Error;

#[cfg(test)]
mod vp8_test;

const VP8HEADER_SIZE: isize = 1;

pub struct VP8Payloader;

impl Payloader for VP8Payloader {
    fn payload<R: Read>(&self, mtu: isize, reader: &mut R) -> Result<Vec<Vec<u8>>, Error> {
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

        let max_fragment_size = mtu - VP8HEADER_SIZE;

        let mut payload_data = vec![];
        reader.read_to_end(&mut payload_data)?;
        let mut payload_data_remaining = payload_data.len() as isize;

        let mut payload_data_index: usize = 0;
        let mut payloads = vec![];

        // Make sure the fragment/payload size is correct
        if std::cmp::min(max_fragment_size, payload_data_remaining) <= 0 {
            return Ok(payloads);
        }

        while payload_data_remaining > 0 {
            let current_fragment_size =
                std::cmp::min(max_fragment_size, payload_data_remaining) as usize;
            let mut out = vec![];
            if payload_data_index == 0 {
                out.push(0x10);
            }

            out.extend_from_slice(
                &payload_data[payload_data_index..payload_data_index + current_fragment_size],
            );
            payloads.push(out);

            payload_data_remaining -= current_fragment_size as isize;
            payload_data_index += current_fragment_size;
        }

        Ok(payloads)
    }
}

#[derive(Debug, Default)]
struct VP8Packet {
    // Required Header
    x: u8,   /* extended controlbits present */
    n: u8,   /* (non-reference frame)  when set to 1 this frame can be discarded */
    s: u8,   /* start of VP8 partition */
    pid: u8, /* partition index */

    // Optional Header
    i: u8, /* 1 if PictureID is present */
    l: u8, /* 1 if TL0PICIDX is present */
    t: u8, /* 1 if TID is present */
    k: u8, /* 1 if KEYIDX is present */

    picture_id: u16, /* 8 or 16 bits, picture ID */
    tl0_pic_idx: u8, /* 8 bits temporal level zero index */

    tid: u8,
    y: u8,
    key_idx: u8,

    payload: Vec<u8>,
}

impl Depacketizer for VP8Packet {
    fn depacketize<R: Read>(&mut self, reader: &mut R) -> Result<(), Error> {
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

        self.payload.clear();

        let mut b = reader.read_u8()?;

        self.x = (b & 0x80) >> 7;
        self.n = (b & 0x20) >> 5;
        self.s = (b & 0x10) >> 4;
        self.pid = b & 0x07;

        if self.x == 1 {
            b = reader.read_u8()?;
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
            b = reader.read_u8()?;
            // PID present?
            if b & 0x80 > 0 {
                // M == 1, PID is 16bit
                self.picture_id = (((b & 0x7f) as u16) << 8) | (reader.read_u8()? as u16);
            } else {
                self.picture_id = b as u16;
            }
        }

        if self.l == 1 {
            self.tl0_pic_idx = reader.read_u8()?;
        }

        if self.t == 1 || self.k == 1 {
            b = reader.read_u8()?;
            self.tid = (b & 0b11000000) >> 6;
            self.y = (b & 0b00100000) >> 5;
            self.key_idx = b & 0b00011111;
        }

        reader.read_to_end(&mut self.payload)?;

        if self.payload.is_empty() {
            Err(Error::new("Payload is not large enough".to_string()))
        } else {
            Ok(())
        }
    }
}
