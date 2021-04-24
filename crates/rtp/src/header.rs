use crate::error::*;

use crate::packetizer::Marshaller;
use bytes::{Buf, BufMut, Bytes, BytesMut};

pub const HEADER_LENGTH: usize = 4;
pub const VERSION_SHIFT: u8 = 6;
pub const VERSION_MASK: u8 = 0x3;
pub const PADDING_SHIFT: u8 = 5;
pub const PADDING_MASK: u8 = 0x1;
pub const EXTENSION_SHIFT: u8 = 4;
pub const EXTENSION_MASK: u8 = 0x1;
pub const EXTENSION_PROFILE_ONE_BYTE: u16 = 0xBEDE;
pub const EXTENSION_PROFILE_TWO_BYTE: u16 = 0x1000;
pub const EXTENSION_ID_RESERVED: u8 = 0xF;
pub const CC_MASK: u8 = 0xF;
pub const MARKER_SHIFT: u8 = 7;
pub const MARKER_MASK: u8 = 0x1;
pub const PT_MASK: u8 = 0x7F;
pub const SEQ_NUM_OFFSET: usize = 2;
pub const SEQ_NUM_LENGTH: usize = 2;
pub const TIMESTAMP_OFFSET: usize = 4;
pub const TIMESTAMP_LENGTH: usize = 4;
pub const SSRC_OFFSET: usize = 8;
pub const SSRC_LENGTH: usize = 4;
pub const CSRC_OFFSET: usize = 12;
pub const CSRC_LENGTH: usize = 4;

#[derive(Debug, Eq, PartialEq, Default)]
pub struct Extension {
    pub id: u8,
    pub payload: Bytes,
}

/// Header represents an RTP packet header
/// NOTE: PayloadOffset is populated by Marshal/Unmarshal and should not be modified
#[derive(Debug, Eq, PartialEq, Default)]
pub struct Header {
    pub version: u8,
    pub padding: bool,
    pub extension: bool,
    pub marker: bool,
    pub payload_type: u8,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub csrc: Vec<u32>,
    pub extension_profile: u16,
    pub extensions: Vec<Extension>,
}

impl Marshaller for Header {
    /// Unmarshal parses the passed byte slice and stores the result in the Header this method is called upon
    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error> {
        if raw_packet.len() < HEADER_LENGTH {
            return Err(Error::ErrHeaderSizeInsufficient);
        }
        /*
         *  0                   1                   2                   3
         *  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |V=2|P|X|  CC   |M|     PT      |       sequence number         |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |                           timestamp                           |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |           synchronization source (SSRC) identifier            |
         * +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * |            contributing source (CSRC) identifiers             |
         * |                             ....                              |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */
        let reader = &mut raw_packet.clone();

        let b0 = reader.get_u8();
        let version = b0 >> VERSION_SHIFT & VERSION_MASK;
        let padding = (b0 >> PADDING_SHIFT & PADDING_MASK) > 0;
        let extension = (b0 >> EXTENSION_SHIFT & EXTENSION_MASK) > 0;
        let cc = (b0 & CC_MASK) as usize;

        let mut curr_offset = CSRC_OFFSET + (cc * CSRC_LENGTH);
        if raw_packet.len() < curr_offset {
            return Err(Error::ErrHeaderSizeInsufficient);
        }

        let b1 = reader.get_u8();
        let marker = (b1 >> MARKER_SHIFT & MARKER_MASK) > 0;
        let payload_type = b1 & PT_MASK;

        let sequence_number = reader.get_u16();
        let timestamp = reader.get_u32();
        let ssrc = reader.get_u32();

        let mut csrc = vec![];
        for _ in 0..cc {
            csrc.push(reader.get_u32());
        }

        let (extension_profile, extensions) = if extension {
            let expected = curr_offset + 4;
            if raw_packet.len() < expected {
                return Err(Error::ErrHeaderSizeInsufficientForExtension);
            }
            let extension_profile = reader.get_u16();
            curr_offset += 2;
            let extension_length = reader.get_u16() as usize * 4;
            curr_offset += 2;

            let expected = curr_offset + extension_length;
            if raw_packet.len() < expected {
                return Err(Error::ErrHeaderSizeInsufficientForExtension);
            }

            let mut extensions = vec![];
            match extension_profile {
                // RFC 8285 RTP One Byte Header Extension
                EXTENSION_PROFILE_ONE_BYTE => {
                    let end = curr_offset + extension_length;
                    while curr_offset < end {
                        let b = reader.get_u8();
                        if b == 0x00 {
                            // padding
                            curr_offset += 1;
                            continue;
                        }

                        let extid = b >> 4;
                        let len = ((b & (0xFF ^ 0xF0)) + 1) as usize;
                        curr_offset += 1;

                        if extid == EXTENSION_ID_RESERVED {
                            break;
                        }

                        extensions.push(Extension {
                            id: extid,
                            payload: raw_packet.slice(curr_offset..curr_offset + len),
                        });
                        reader.advance(len);
                        curr_offset += len;
                    }
                }
                // RFC 8285 RTP Two Byte Header Extension
                EXTENSION_PROFILE_TWO_BYTE => {
                    let end = curr_offset + extension_length;
                    while curr_offset < end {
                        let b = reader.get_u8();
                        if b == 0x00 {
                            // padding
                            curr_offset += 1;
                            continue;
                        }

                        let extid = b;
                        curr_offset += 1;

                        let len = reader.get_u8() as usize;
                        curr_offset += 1;

                        extensions.push(Extension {
                            id: extid,
                            payload: raw_packet.slice(curr_offset..curr_offset + len),
                        });
                        reader.advance(len);
                        curr_offset += len;
                    }
                }
                // RFC3550 Extension
                _ => {
                    if raw_packet.len() < curr_offset + extension_length {
                        return Err(Error::ErrHeaderSizeInsufficientForExtension);
                    }
                    extensions.push(Extension {
                        id: 0,
                        payload: raw_packet.slice(curr_offset..curr_offset + extension_length),
                    });
                    reader.advance(extension_length);
                    //curr_offset += extension_length;
                }
            };

            (extension_profile, extensions)
        } else {
            (0, vec![])
        };

        Ok(Header {
            version,
            padding,
            extension,
            marker,
            payload_type,
            sequence_number,
            timestamp,
            ssrc,
            csrc,
            extension_profile,
            extensions,
        })
    }

    /// MarshalSize returns the size of the packet once marshaled.
    fn marshal_size(&self) -> usize {
        let mut head_size = 12 + (self.csrc.len() * CSRC_LENGTH);
        if self.extension {
            let extension_payload_len = self.get_extension_payload_len();
            let extension_payload_size = (extension_payload_len + 3) / 4;
            head_size += 4 + extension_payload_size * 4;
        }
        head_size
    }

    /// Marshal serializes the header and writes to the buffer.
    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error> {
        /*
         *  0                   1                   2                   3
         *  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |V=2|P|X|  CC   |M|     PT      |       sequence number         |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |                           timestamp                           |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |           synchronization source (SSRC) identifier            |
         * +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * |            contributing source (CSRC) identifiers             |
         * |                             ....                              |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */
        let writer = buf;

        // The first byte contains the version, padding bit, extension bit, and csrc size
        let mut b0 = (self.version << VERSION_SHIFT) | self.csrc.len() as u8;
        if self.padding {
            b0 |= 1 << PADDING_SHIFT;
        }

        if self.extension {
            b0 |= 1 << EXTENSION_SHIFT;
        }
        writer.put_u8(b0);

        // The second byte contains the marker bit and payload type.
        let mut b1 = self.payload_type;
        if self.marker {
            b1 |= 1 << MARKER_SHIFT;
        }
        writer.put_u8(b1);

        writer.put_u16(self.sequence_number);
        writer.put_u32(self.timestamp);
        writer.put_u32(self.ssrc);

        let mut n = 12;
        for csrc in &self.csrc {
            writer.put_u32(*csrc);
            n += 4;
        }

        if self.extension {
            writer.put_u16(self.extension_profile);
            n += 2;

            // calculate extensions size and round to 4 bytes boundaries
            let extension_payload_len = self.get_extension_payload_len();
            if self.extension_profile != EXTENSION_PROFILE_ONE_BYTE
                && self.extension_profile != EXTENSION_PROFILE_TWO_BYTE
                && extension_payload_len % 4 != 0
            {
                //the payload must be in 32-bit words.
                return Err(Error::HeaderExtensionPayloadNot32BitWords);
            }
            let extension_payload_size = (extension_payload_len as u16 + 3) / 4;
            writer.put_u16(extension_payload_size);
            n += 2;

            match self.extension_profile {
                // RFC 8285 RTP One Byte Header Extension
                EXTENSION_PROFILE_ONE_BYTE => {
                    for extension in &self.extensions {
                        writer.put_u8((extension.id << 4) | (extension.payload.len() as u8 - 1));
                        n += 1;
                        writer.put(&*extension.payload);
                        n += extension.payload.len();
                    }
                }
                // RFC 8285 RTP Two Byte Header Extension
                EXTENSION_PROFILE_TWO_BYTE => {
                    for extension in &self.extensions {
                        writer.put_u8(extension.id);
                        n += 1;
                        writer.put_u8(extension.payload.len() as u8);
                        n += 1;
                        writer.put(&*extension.payload);
                        n += extension.payload.len();
                    }
                }
                // RFC3550 Extension
                _ => {
                    if self.extensions.len() != 1 {
                        return Err(Error::ErrRfc3550headerIdrange);
                    }

                    if let Some(extension) = self.extensions.first() {
                        let ext_len = extension.payload.len();
                        if ext_len % 4 != 0 {
                            return Err(Error::HeaderExtensionPayloadNot32BitWords);
                        }
                        writer.put(&*extension.payload);
                        n += ext_len;
                    }
                }
            };

            // add padding to reach 4 bytes boundaries
            for _ in extension_payload_len..extension_payload_size as usize * 4 {
                writer.put_u8(0);
                n += 1;
            }
        }

        Ok(n)
    }
}

impl Header {
    pub fn get_extension_payload_len(&self) -> usize {
        let mut extension_length = 0;
        match self.extension_profile {
            EXTENSION_PROFILE_ONE_BYTE => {
                for extension in &self.extensions {
                    extension_length += 1 + extension.payload.len();
                }
            }
            EXTENSION_PROFILE_TWO_BYTE => {
                for extension in &self.extensions {
                    extension_length += 2 + extension.payload.len();
                }
            }
            _ => {
                for extension in &self.extensions {
                    extension_length += extension.payload.len();
                }
            }
        };

        extension_length
    }

    /// SetExtension sets an RTP header extension
    pub fn set_extension(&mut self, id: u8, payload: Bytes) -> Result<(), Error> {
        if self.extension {
            match self.extension_profile {
                EXTENSION_PROFILE_ONE_BYTE => {
                    if !(1..=14).contains(&id) {
                        return Err(Error::ErrRfc8285oneByteHeaderIdrange);
                    }
                    if payload.len() > 16 {
                        return Err(Error::ErrRfc8285oneByteHeaderSize);
                    }
                }
                EXTENSION_PROFILE_TWO_BYTE => {
                    if id < 1 {
                        return Err(Error::ErrRfc8285twoByteHeaderIdrange);
                    }
                    if payload.len() > 255 {
                        return Err(Error::ErrRfc8285twoByteHeaderSize);
                    }
                }
                _ => {
                    if id != 0 {
                        return Err(Error::ErrRfc3550headerIdrange);
                    }
                }
            };

            // Update existing if it exists else add new extension
            for extension in &mut self.extensions {
                if extension.id == id {
                    extension.payload = payload;
                    return Ok(());
                }
            }
            self.extensions.push(Extension { id, payload });
            return Ok(());
        }

        // No existing header extensions
        self.extension = true;

        let len = payload.len();
        if len <= 16 {
            self.extension_profile = EXTENSION_PROFILE_ONE_BYTE
        } else if len > 16 && len < 256 {
            self.extension_profile = EXTENSION_PROFILE_TWO_BYTE
        }

        self.extensions.push(Extension { id, payload });

        Ok(())
    }

    /// returns an RTP header extension
    pub fn get_extension(&self, id: u8) -> Option<Bytes> {
        if !self.extension {
            return None;
        }

        for extension in &self.extensions {
            if extension.id == id {
                return Some(extension.payload.clone());
            }
        }
        None
    }

    /// Removes an RTP Header extension
    pub fn del_extension(&mut self, id: u8) -> Result<(), Error> {
        if !self.extension {
            return Err(Error::ErrHeaderExtensionsNotEnabled);
        }
        for index in 0..self.extensions.len() {
            if self.extensions[index].id == id {
                self.extensions.remove(index);
                return Ok(());
            }
        }
        Err(Error::ErrHeaderExtensionNotFound)
    }
}
