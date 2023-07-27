//! Based on https://chromium.googlesource.com/external/webrtc/+/4e513346ec56c829b3a6010664998469fc237b35/modules/rtp_rtcp/source/rtp_packetizer_av1.cc
//! Reference: https://aomediacodec.github.io/av1-spec/#obu-syntax

use bytes::Bytes;

use crate::codecs::av1::leb128::read_leb128;
use crate::error::Result;
use crate::Error::{ErrPayloadTooSmallForObuExtensionHeader, ErrPayloadTooSmallForObuPayloadSize};

pub const OBU_HAS_EXTENSION_BIT: u8 = 0b0000_0100;
pub const OBU_HAS_SIZE_BIT: u8 = 0b0000_0010;
pub const OBU_TYPE_MASK: u8 = 0b0111_1000;

pub const OBU_TYPE_SEQUENCE_HEADER: u8 = 1;
pub const OBU_TYPE_TEMPORAL_DELIMITER: u8 = 2;
pub const OBU_TYPE_FRAME_HEADER: u8 = 3;
pub const OBU_TYPE_TILE_GROUP: u8 = 4;
pub const OBU_TYPE_METADATA: u8 = 5;
pub const OBU_TYPE_FRAME: u8 = 6;
pub const OBU_TYPE_TILE_LIST: u8 = 8;
pub const OBU_TYPE_PADDING: u8 = 15;

#[derive(Debug, Clone)]
pub struct Obu {
    pub header: u8,
    pub extension_header: u8,
    pub payload: Bytes,
    /// size of the header and payload combined.
    pub size: usize,
}

impl Obu {
    pub fn header_size(&self) -> usize {
        if obu_has_extension(self.header) {
            2
        } else {
            1
        }
    }
}

/// Parses the payload into series of OBUs.
/// Reference: https://aomediacodec.github.io/av1-spec/#obu-syntax
pub fn parse_obus(payload: &Bytes) -> Result<Vec<Obu>> {
    let mut obus = vec![];
    let mut payload_data_remaining = payload.len() as isize;
    let mut payload_data_index: usize = 0;

    while payload_data_remaining > 0 {
        // Read OBU header.
        let header = payload[payload_data_index];
        let has_extension = obu_has_extension(header);
        let has_size = obu_has_size(header);
        let obu_type = obu_type(header);

        // Read OBU extension header.
        let extension_header = if has_extension {
            if payload_data_remaining < 2 {
                return Err(ErrPayloadTooSmallForObuExtensionHeader);
            }
            payload[payload_data_index + 1]
        } else {
            0
        };
        let obu_header_size = if has_extension { 2 } else { 1 };
        let payload_without_header = payload.slice(payload_data_index + obu_header_size..);

        // Read OBU payload.
        let obu_payload = if !has_size {
            payload_without_header
        } else {
            if payload_without_header.is_empty() {
                return Err(ErrPayloadTooSmallForObuPayloadSize);
            }
            let (obu_payload_size, leb128_size) = read_leb128(&payload_without_header);
            payload_data_remaining -= leb128_size as isize;
            payload_data_index += leb128_size;
            payload_without_header.slice(leb128_size..leb128_size + obu_payload_size as usize)
        };

        let obu_size = obu_header_size + obu_payload.len();
        if !should_ignore_obu_type(obu_type) {
            obus.push(Obu {
                header,
                extension_header,
                payload: obu_payload,
                size: obu_size,
            });
        }

        payload_data_remaining -= obu_size as isize;
        payload_data_index += obu_size;
    }

    Ok(obus)
}

pub fn obu_has_extension(header: u8) -> bool {
    header & OBU_HAS_EXTENSION_BIT != 0
}

pub fn obu_has_size(header: u8) -> bool {
    header & OBU_HAS_SIZE_BIT != 0
}

pub fn obu_type(header: u8) -> u8 {
    (header & OBU_TYPE_MASK) >> 3
}

fn should_ignore_obu_type(obu_type: u8) -> bool {
    obu_type == OBU_TYPE_TEMPORAL_DELIMITER
        || obu_type == OBU_TYPE_TILE_LIST
        || obu_type == OBU_TYPE_PADDING
}
