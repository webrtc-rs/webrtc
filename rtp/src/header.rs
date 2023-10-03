use bytes::{Buf, BufMut, Bytes};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;

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

#[derive(Debug, Eq, PartialEq, Default, Clone)]
pub struct Extension {
    pub id: u8,
    pub payload: Bytes,
}

/// Header represents an RTP packet header
/// NOTE: PayloadOffset is populated by Marshal/Unmarshal and should not be modified
#[derive(Debug, Eq, PartialEq, Default, Clone)]
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
    pub extensions_padding: usize,
}

impl Unmarshal for Header {
    /// Unmarshal parses the passed byte slice and stores the result in the Header this method is called upon
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self, util::Error>
    where
        Self: Sized,
        B: Buf,
    {
        let raw_packet_len = raw_packet.remaining();
        if raw_packet_len < HEADER_LENGTH {
            return Err(Error::ErrHeaderSizeInsufficient.into());
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
        let b0 = raw_packet.get_u8();
        let version = b0 >> VERSION_SHIFT & VERSION_MASK;
        let padding = (b0 >> PADDING_SHIFT & PADDING_MASK) > 0;
        let extension = (b0 >> EXTENSION_SHIFT & EXTENSION_MASK) > 0;
        let cc = (b0 & CC_MASK) as usize;

        let mut curr_offset = CSRC_OFFSET + (cc * CSRC_LENGTH);
        if raw_packet_len < curr_offset {
            return Err(Error::ErrHeaderSizeInsufficient.into());
        }

        let b1 = raw_packet.get_u8();
        let marker = (b1 >> MARKER_SHIFT & MARKER_MASK) > 0;
        let payload_type = b1 & PT_MASK;

        let sequence_number = raw_packet.get_u16();
        let timestamp = raw_packet.get_u32();
        let ssrc = raw_packet.get_u32();

        let mut csrc = Vec::with_capacity(cc);
        for _ in 0..cc {
            csrc.push(raw_packet.get_u32());
        }
        let mut extensions_padding: usize = 0;
        let (extension_profile, extensions) = if extension {
            let expected = curr_offset + 4;
            if raw_packet_len < expected {
                return Err(Error::ErrHeaderSizeInsufficientForExtension.into());
            }
            let extension_profile = raw_packet.get_u16();
            curr_offset += 2;
            let extension_length = raw_packet.get_u16() as usize * 4;
            curr_offset += 2;

            let expected = curr_offset + extension_length;
            if raw_packet_len < expected {
                return Err(Error::ErrHeaderSizeInsufficientForExtension.into());
            }

            let mut extensions = vec![];
            match extension_profile {
                // RFC 8285 RTP One Byte Header Extension
                EXTENSION_PROFILE_ONE_BYTE => {
                    let end = curr_offset + extension_length;
                    while curr_offset < end {
                        let b = raw_packet.get_u8();
                        if b == 0x00 {
                            // padding
                            curr_offset += 1;
                            extensions_padding += 1;
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
                            payload: raw_packet.copy_to_bytes(len),
                        });
                        curr_offset += len;
                    }
                }
                // RFC 8285 RTP Two Byte Header Extension
                EXTENSION_PROFILE_TWO_BYTE => {
                    let end = curr_offset + extension_length;
                    while curr_offset < end {
                        let b = raw_packet.get_u8();
                        if b == 0x00 {
                            // padding
                            curr_offset += 1;
                            extensions_padding += 1;
                            continue;
                        }

                        let extid = b;
                        curr_offset += 1;

                        let len = raw_packet.get_u8() as usize;
                        curr_offset += 1;

                        extensions.push(Extension {
                            id: extid,
                            payload: raw_packet.copy_to_bytes(len),
                        });
                        curr_offset += len;
                    }
                }
                // RFC3550 Extension
                _ => {
                    if raw_packet_len < curr_offset + extension_length {
                        return Err(Error::ErrHeaderSizeInsufficientForExtension.into());
                    }
                    extensions.push(Extension {
                        id: 0,
                        payload: raw_packet.copy_to_bytes(extension_length),
                    });
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
            extensions_padding,
        })
    }
}

impl MarshalSize for Header {
    /// MarshalSize returns the size of the packet once marshaled.
    fn marshal_size(&self) -> usize {
        let mut head_size = 12 + (self.csrc.len() * CSRC_LENGTH);
        if self.extension {
            let extension_payload_len = self.get_extension_payload_len() + self.extensions_padding;
            let extension_payload_size = (extension_payload_len + 3) / 4;
            head_size += 4 + extension_payload_size * 4;
        }
        head_size
    }
}

impl Marshal for Header {
    /// Marshal serializes the header and writes to the buffer.
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize, util::Error> {
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
        let remaining_before = buf.remaining_mut();
        if remaining_before < self.marshal_size() {
            return Err(Error::ErrBufferTooSmall.into());
        }

        // The first byte contains the version, padding bit, extension bit, and csrc size
        let mut b0 = (self.version << VERSION_SHIFT) | self.csrc.len() as u8;
        if self.padding {
            b0 |= 1 << PADDING_SHIFT;
        }

        if self.extension {
            b0 |= 1 << EXTENSION_SHIFT;
        }
        buf.put_u8(b0);

        // The second byte contains the marker bit and payload type.
        let mut b1 = self.payload_type;
        if self.marker {
            b1 |= 1 << MARKER_SHIFT;
        }
        buf.put_u8(b1);

        buf.put_u16(self.sequence_number);
        buf.put_u32(self.timestamp);
        buf.put_u32(self.ssrc);

        for csrc in &self.csrc {
            buf.put_u32(*csrc);
        }

        if self.extension {
            buf.put_u16(self.extension_profile);

            // calculate extensions size and round to 4 bytes boundaries
            let extension_payload_len = self.get_extension_payload_len();
            if self.extension_profile != EXTENSION_PROFILE_ONE_BYTE
                && self.extension_profile != EXTENSION_PROFILE_TWO_BYTE
                && extension_payload_len % 4 != 0
            {
                //the payload must be in 32-bit words.
                return Err(Error::HeaderExtensionPayloadNot32BitWords.into());
            }
            let extension_payload_size = (extension_payload_len as u16 + 3) / 4;
            buf.put_u16(extension_payload_size);

            match self.extension_profile {
                // RFC 8285 RTP One Byte Header Extension
                EXTENSION_PROFILE_ONE_BYTE => {
                    for extension in &self.extensions {
                        buf.put_u8((extension.id << 4) | (extension.payload.len() as u8 - 1));
                        buf.put(&*extension.payload);
                    }
                }
                // RFC 8285 RTP Two Byte Header Extension
                EXTENSION_PROFILE_TWO_BYTE => {
                    for extension in &self.extensions {
                        buf.put_u8(extension.id);
                        buf.put_u8(extension.payload.len() as u8);
                        buf.put(&*extension.payload);
                    }
                }
                // RFC3550 Extension
                _ => {
                    if self.extensions.len() != 1 {
                        return Err(Error::ErrRfc3550headerIdrange.into());
                    }

                    if let Some(extension) = self.extensions.first() {
                        let ext_len = extension.payload.len();
                        if ext_len % 4 != 0 {
                            return Err(Error::HeaderExtensionPayloadNot32BitWords.into());
                        }
                        buf.put(&*extension.payload);
                    }
                }
            };

            // add padding to reach 4 bytes boundaries
            for _ in extension_payload_len..extension_payload_size as usize * 4 {
                buf.put_u8(0);
            }
        }

        let remaining_after = buf.remaining_mut();
        Ok(remaining_before - remaining_after)
    }
}

impl Header {
    pub fn get_extension_payload_len(&self) -> usize {
        let payload_len: usize = self
            .extensions
            .iter()
            .map(|extension| extension.payload.len())
            .sum();

        let profile_len = self.extensions.len()
            * match self.extension_profile {
                EXTENSION_PROFILE_ONE_BYTE => 1,
                EXTENSION_PROFILE_TWO_BYTE => 2,
                _ => 0,
            };

        payload_len + profile_len
    }

    /// SetExtension sets an RTP header extension
    pub fn set_extension(&mut self, id: u8, payload: Bytes) -> Result<(), Error> {
        let payload_len = payload.len() as isize;
        if self.extension {
            let extension_profile_len = match self.extension_profile {
                EXTENSION_PROFILE_ONE_BYTE => {
                    if !(1..=14).contains(&id) {
                        return Err(Error::ErrRfc8285oneByteHeaderIdrange);
                    }
                    if payload_len > 16 {
                        return Err(Error::ErrRfc8285oneByteHeaderSize);
                    }
                    1
                }
                EXTENSION_PROFILE_TWO_BYTE => {
                    if id < 1 {
                        return Err(Error::ErrRfc8285twoByteHeaderIdrange);
                    }
                    if payload_len > 255 {
                        return Err(Error::ErrRfc8285twoByteHeaderSize);
                    }
                    2
                }
                _ => {
                    if id != 0 {
                        return Err(Error::ErrRfc3550headerIdrange);
                    }
                    0
                }
            };

            let delta;
            // Update existing if it exists else add new extension
            if let Some(extension) = self
                .extensions
                .iter_mut()
                .find(|extension| extension.id == id)
            {
                delta = payload_len - extension.payload.len() as isize;
                extension.payload = payload;
            } else {
                delta = payload_len + extension_profile_len;
                self.extensions.push(Extension { id, payload });
            }

            match delta.cmp(&0) {
                std::cmp::Ordering::Less => {
                    self.extensions_padding =
                        ((self.extensions_padding as isize - delta) % 4) as usize;
                }
                std::cmp::Ordering::Greater => {
                    let extension_padding = (delta % 4) as usize;
                    if self.extensions_padding < extension_padding {
                        self.extensions_padding = (self.extensions_padding + 4) - extension_padding;
                    } else {
                        self.extensions_padding -= extension_padding
                    }
                }
                _ => {}
            }
        } else {
            // No existing header extensions
            self.extension = true;
            let mut extension_profile_len = 0;
            self.extension_profile = match payload_len {
                0..=16 => {
                    extension_profile_len = 1;
                    EXTENSION_PROFILE_ONE_BYTE
                }
                17..=255 => {
                    extension_profile_len = 2;
                    EXTENSION_PROFILE_TWO_BYTE
                }
                _ => self.extension_profile,
            };

            let extension_padding = (payload.len() + extension_profile_len) % 4;
            if self.extensions_padding < extension_padding {
                self.extensions_padding = self.extensions_padding + 4 - extension_padding;
            } else {
                self.extensions_padding -= extension_padding
            }
            self.extensions.push(Extension { id, payload });
        }
        Ok(())
    }

    /// returns an extension id array
    pub fn get_extension_ids(&self) -> Vec<u8> {
        if self.extension {
            self.extensions.iter().map(|e| e.id).collect()
        } else {
            vec![]
        }
    }

    /// returns an RTP header extension
    pub fn get_extension(&self, id: u8) -> Option<Bytes> {
        if self.extension {
            self.extensions
                .iter()
                .find(|extension| extension.id == id)
                .map(|extension| extension.payload.clone())
        } else {
            None
        }
    }

    /// Removes an RTP Header extension
    pub fn del_extension(&mut self, id: u8) -> Result<(), Error> {
        if self.extension {
            if let Some(index) = self
                .extensions
                .iter()
                .position(|extension| extension.id == id)
            {
                let extension = self.extensions.remove(index);

                let extension_profile_len = match self.extension_profile {
                    EXTENSION_PROFILE_ONE_BYTE => 1,
                    EXTENSION_PROFILE_TWO_BYTE => 2,
                    _ => 0,
                };

                let extension_padding = (extension.payload.len() + extension_profile_len) % 4;
                self.extensions_padding = (self.extensions_padding + extension_padding) % 4;

                Ok(())
            } else {
                Err(Error::ErrHeaderExtensionNotFound)
            }
        } else {
            Err(Error::ErrHeaderExtensionsNotEnabled)
        }
    }
}
