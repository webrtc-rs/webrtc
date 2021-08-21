#[cfg(test)]
mod vp9_test;

use crate::{
    error::Error,
    packetizer::{Depacketizer, Payloader},
};

use anyhow::Result;
use bytes::{Buf, Bytes};

/// Vp9Payloader payloads VP9 packets
#[derive(Debug, Copy, Clone)]
pub struct Vp9Payloader;

impl Payloader for Vp9Payloader {
    /// Payload fragments an Vp9Payloader packet across one or more byte arrays
    fn payload(&self, mtu: usize, payload: &Bytes) -> Result<Vec<Bytes>> {
        if payload.is_empty() || mtu == 0 {
            return Ok(vec![]);
        }

        let mut payloads = vec![];
        let mut payload_data_remaining = payload.len();
        let mut payload_data_index = 0;
        while payload_data_remaining > 0 {
            let current_fragment_size = std::cmp::min(mtu as usize, payload_data_remaining);
            payloads.push(
                payload.slice(payload_data_index..payload_data_index + current_fragment_size),
            );

            payload_data_remaining -= current_fragment_size;
            payload_data_index += current_fragment_size;
        }

        Ok(payloads)
    }

    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync> {
        Box::new(*self)
    }
}

/// Vp9Packet represents the VP9 header that is stored in the payload of an RTP Packet
#[derive(PartialEq, Debug, Default, Clone)]
pub struct Vp9Packet {
    pub i: u8, // picture ID is present
    pub p: u8, // inter-picture predicted frame.
    pub l: u8, // layer indices present
    pub f: u8, // flexible mode
    pub b: u8, // start of frame. beginning of new vp9 frame
    pub e: u8, // end of frame
    pub v: u8, // scalability structure (SS) present
    pub z: u8, // not a reference frame for higher spatial layers

    // 7 or 15 bits, picture ID.
    pub picture_id: u16,

    // present if l == 1
    pub layer_index: Option<Vp9LayerIndex>,

    // present if p == 1 and f == 1. 1-3 reference indexes. 0 means not used.
    pub p_diff: Vec<u8>,

    // present if v == 1
    pub scalability_structure: Option<Vp9ScalabilityStructure>,

    pub payload: Bytes,
}

/// Represents the layer index part of a [Vp9Packet] in an RTP packet.
#[derive(PartialEq, Debug, Default, Clone)]
pub struct Vp9LayerIndex {
    pub tid: u8,
    pub u: u8,
    pub sid: u8,
    pub d: u8,
    // present if f == 0. temporal level zero index.
    pub tl0_pic_idx: u8,
}

/// Represents the Scalability Structure (SS) of a [Vp9Packet] in an RTP packet.
#[derive(PartialEq, Debug, Default, Clone)]
pub struct Vp9ScalabilityStructure {
    pub ns: u8,
    pub y: u8,
    pub g: u8,

    // repeated ns + 1 times when y == 1
    pub frame_resolution: Vec<Vp9FrameResolution>,

    // present if g == 1
    pub ng: u8,

    // repeated ng times when g == 1
    pub picture_group: Vec<Vp9PictureGroup>,
}

/// Frame resolution inside a [Vp9ScalabilityStructure].
#[derive(PartialEq, Debug, Default, Clone)]
pub struct Vp9FrameResolution {
    pub width: u16,
    pub height: u16,
}

/// Picture group as part of [Vp9ScalabilityStructure].
#[derive(PartialEq, Debug, Default, Clone)]
pub struct Vp9PictureGroup {
    pub tid: u8,
    pub u: u8,
    pub r: u8,

    // r number of P_DIFF.
    pub p_diff: Vec<u8>,
}

impl Depacketizer for Vp9Packet {
    /// depacketize parses the passed byte slice and stores the result in the Vp9Packet this method is called upon
    fn depacketize(&mut self, packet: &Bytes) -> Result<()> {
        if packet.len() < 2 {
            return Err(Error::ErrShortPacket.into());
        }

        // Doc here: https://datatracker.ietf.org/doc/html/draft-ietf-payload-vp9-16

        // Flexible mode:
        //          0 1 2 3 4 5 6 7
        //         +-+-+-+-+-+-+-+-+
        //         |I|P|L|F|B|E|V|Z| (REQUIRED)
        //         +-+-+-+-+-+-+-+-+
        //    I:   |M| PICTURE ID  | (REQUIRED)
        //         +-+-+-+-+-+-+-+-+
        //    M:   | EXTENDED PID  | (RECOMMENDED)
        //         +-+-+-+-+-+-+-+-+
        //    L:   | TID |U| SID |D| (Conditionally RECOMMENDED)
        //         +-+-+-+-+-+-+-+-+                             -\
        //    P,F: | P_DIFF      |N| (Conditionally REQUIRED)    - up to 3 times
        //         +-+-+-+-+-+-+-+-+                             -/
        //    V:   | SS            |
        //         | ..            |
        //         +-+-+-+-+-+-+-+-+

        // Non-flexible mode:
        //          0 1 2 3 4 5 6 7
        //         +-+-+-+-+-+-+-+-+
        //         |I|P|L|F|B|E|V|Z| (REQUIRED)
        //         +-+-+-+-+-+-+-+-+
        //    I:   |M| PICTURE ID  | (RECOMMENDED)
        //         +-+-+-+-+-+-+-+-+
        //    M:   | EXTENDED PID  | (RECOMMENDED)
        //         +-+-+-+-+-+-+-+-+
        //    L:   | TID |U| SID |D| (Conditionally RECOMMENDED)
        //         +-+-+-+-+-+-+-+-+
        //         |   TL0PICIDX   | (Conditionally REQUIRED)
        //         +-+-+-+-+-+-+-+-+
        //    V:   | SS            |
        //         | ..            |
        //         +-+-+-+-+-+-+-+-+

        let reader = &mut packet.clone();
        let mut payload_index = 0;

        let mut b = reader.get_u8();
        payload_index += 1;

        self.i = (b & 0x80) >> 7;
        self.p = (b & 0x40) >> 6;
        self.l = (b & 0x20) >> 5;
        self.f = (b & 0x10) >> 4;
        self.b = (b & 0x08) >> 3;
        self.e = (b & 0x04) >> 2;
        self.v = (b & 0x02) >> 1;
        self.z = b & 0x01;

        if self.i == 1 {
            b = reader.get_u8();
            payload_index += 1;
            // PID present?
            if b & 0x80 > 0 {
                // M == 1, PID is 15bit
                self.picture_id = (((b & 0x7f) as u16) << 8) | (reader.get_u8() as u16);
                payload_index += 1;
            } else {
                self.picture_id = b as u16;
            }
        }

        if self.l == 1 {
            let mut l = Vp9LayerIndex::default();

            b = reader.get_u8();
            payload_index += 1;

            l.tid = (b & 0b1110_0000) >> 5;
            l.u = (b & 0b0001_0000) >> 4;
            l.sid = (b & 0b0000_1110) >> 1;
            l.d = b & 0b0000_0001;

            if self.f == 0 {
                // non-flexible mode requires TL0PICIDX
                l.tl0_pic_idx = reader.get_u8();
                payload_index += 1;
            }

            self.layer_index = Some(l);
        }

        if self.f == 1 && self.p == 1 {
            loop {
                b = reader.get_u8();
                payload_index += 1;

                self.p_diff.push((b & 0b1111_1110) >> 1);

                let has_more = (b & 1) > 0;

                if !has_more || self.p_diff.len() == 3 {
                    break;
                }
            }
        }

        if self.v == 1 {
            //         +-+-+-+-+-+-+-+-+
            //    V:   | N_S |Y|G|-|-|-|
            //         +-+-+-+-+-+-+-+-+              -\
            //    Y:   |     WIDTH     | (OPTIONAL)    .
            //         +               +               .
            //         |               | (OPTIONAL)    .
            //         +-+-+-+-+-+-+-+-+               . - N_S + 1 times
            //         |     HEIGHT    | (OPTIONAL)    .
            //         +               +               .
            //         |               | (OPTIONAL)    .
            //         +-+-+-+-+-+-+-+-+              -/
            //    G:   |      N_G      | (OPTIONAL)
            //         +-+-+-+-+-+-+-+-+                            -\
            //    N_G: | TID |U| R |-|-| (OPTIONAL)                 .
            //         +-+-+-+-+-+-+-+-+              -\            . - N_G times
            //         |    P_DIFF     | (OPTIONAL)    . - R times  .
            //         +-+-+-+-+-+-+-+-+              -/            -/

            let mut s = Vp9ScalabilityStructure::default();

            b = reader.get_u8();
            payload_index += 1;

            s.ns = (b & 0b1110_0000) >> 5;
            s.y = (b & 0b0001_0000) >> 4;
            s.g = (b & 0b0000_1000) >> 3;

            if s.y == 1 {
                for _ in 0..(s.ns + 1) {
                    s.frame_resolution.push(Vp9FrameResolution {
                        width: reader.get_u16(),
                        height: reader.get_u16(),
                    });
                    payload_index += 4;
                }
            }

            if s.g == 1 {
                s.ng = reader.get_u8();
                payload_index += 1;

                for _ in 0..s.ng {
                    b = reader.get_u8();
                    payload_index += 1;

                    let mut p = Vp9PictureGroup {
                        tid: (b & 0b1110_0000) >> 5,
                        u: (b & 0b0001_0000) >> 4,
                        r: (b & 0b0000_1100) >> 2,
                        ..Default::default()
                    };

                    for _ in 0..p.r {
                        p.p_diff.push(reader.get_u8());
                        payload_index += 1;
                    }
                }
            }
        }

        self.payload = packet.slice(payload_index..);

        Ok(())
    }
}
