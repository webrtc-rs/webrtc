use std::{
    io::{Read, Write},
    todo,
};

use util::Error;

use byteorder::{BigEndian, ReadBytesExt};

const HEADER_LENGTH: usize = 4;
const VERSION_SHIFT: u8 = 6;
const VERSION_MASK: u8 = 0x3;
const PADDING_SHIFT: u8 = 5;
const PADDING_MASK: u8 = 0x1;
const EXTENSION_SHIFT: u8 = 4;
const EXTENSION_MASK: u8 = 0x1;
const EXTENSION_ID_RESERVED: u8 = 0xF;
const CC_MASK: u8 = 0xF;
const MARKER_SHIFT: u8 = 7;
const MARKER_MASK: u8 = 0x1;
const PT_MASK: u8 = 0x7F;
const SEQ_NUM_OFFSET: usize = 2;
const SEQ_NUM_LENGTH: usize = 2;
const TIMESTAMP_OFFSET: usize = 4;
const TIMESTAMP_LENGTH: usize = 4;
const SSRC_OFFSET: usize = 8;
const SSRC_LENGTH: usize = 4;
const CSRC_OFFSET: usize = 12;
const CSRC_LENGTH: usize = 4;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u16)]
pub enum ExtensionProfile {
    OneByte = 0xBEDE,
    TwoByte = 0x1000,
    Default,
}

impl Default for ExtensionProfile {
    fn default() -> Self {
        ExtensionProfile::OneByte
    }
}

impl From<u16> for ExtensionProfile {
    fn from(val: u16) -> Self {
        match val {
            0xBEDE => ExtensionProfile::OneByte,
            0x1000 => ExtensionProfile::TwoByte,
            _ => ExtensionProfile::Default,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Default)]
pub struct Extension {
    pub id: u8,
    pub payload: Vec<u8>,
}

// Header represents an RTP packet header
// NOTE: PayloadOffset is populated by Marshal/Unmarshal and should not be modified
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
    pub extension_profile: ExtensionProfile,
    pub extensions: Vec<Extension>,

    pub payload_offset: usize,
}

impl Header {
    // MarshalSize returns the size of the packet once marshaled.
    pub fn marshal_size(&self) -> usize {
        let mut head_size = 12 + (self.csrc.len() * CSRC_LENGTH);
        if self.extension {
            let extension_payload_len = self.get_extension_payload_len();
            let extension_payload_size = (extension_payload_len + 3) / 4;
            head_size += 4 + extension_payload_size * 4;
        }
        head_size
    }

    fn get_extension_payload_len(&self) -> usize {
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

    // SetExtension sets an RTP header extension
    pub fn set_extension(&mut self, id: u8, payload: &[u8]) -> Result<(), Error> {
        if self.extension {
            match self.extension_profile {
                ExtensionProfile::OneByte => {
                    if id < 1 || id > 14 {
                        return Err(Error::new(
                            "header extension id must be between 1 and 14 for RFC 5285 extensions"
                                .to_owned(),
                        ));
                    }
                    if payload.len() > 16 {
                        return Err(Error::new("header extension payload must be 16bytes or less for RFC 5285 one byte extensions".to_owned()));
                    }
                }
                ExtensionProfile::TwoByte => {
                    if id < 1 {
                        return Err(Error::new(
                            "header extension id must be between 1 and 255 for RFC 5285 extensions"
                                .to_owned(),
                        ));
                    }
                    if payload.len() > 255 {
                        return Err(Error::new("header extension payload must be 255bytes or less for RFC 5285 two byte extensions".to_owned()));
                    }
                }
                _ => {
                    if id != 0 {
                        return Err(Error::new(
                            "header extension id must be 0 for none RFC 5285 extensions".to_owned(),
                        ));
                    }
                }
            };

            // Update existing if it exists else add new extension
            for extension in &mut self.extensions {
                if extension.id == id {
                    extension.payload.clear();
                    extension.payload.extend_from_slice(payload);
                    return Ok(());
                }
            }
            self.extensions.push(Extension {
                id,
                payload: payload.to_vec(),
            });
            return Ok(());
        }

        // No existing header extensions
        self.extension = true;

        let len = payload.len();
        if len <= 16 {
            self.extension_profile = ExtensionProfile::OneByte
        } else if len > 16 && len < 256 {
            self.extension_profile = ExtensionProfile::TwoByte
        }

        self.extensions.push(Extension {
            id,
            payload: payload.to_vec(),
        });

        Ok(())
    }

    // returns an RTP header extension
    pub fn get_extension(&self, id: u8) -> Option<&[u8]> {
        if !self.extension {
            return None;
        }

        for extension in &self.extensions {
            if extension.id == id {
                return Some(&extension.payload);
            }
        }
        None
    }

    // Removes an RTP Header extension
    pub fn del_extension(&mut self, id: u8) -> Result<(), Error> {
        if !self.extension {
            return Err(Error::new("extension not enabled".to_owned()));
        }
        for index in 0..self.extensions.len() {
            if self.extensions[index].id == id {
                self.extensions.remove(index);
                return Ok(());
            }
        }
        Err(Error::new("extension not found".to_owned()))
    }

    // Unmarshal parses the passed byte slice and stores the result in the Header this method is called upon
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
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

        let b0 = reader.read_u8()?;
        let version = b0 >> VERSION_SHIFT & VERSION_MASK;
        let padding = (b0 >> PADDING_SHIFT & PADDING_MASK) > 0;
        let extension = (b0 >> EXTENSION_SHIFT & EXTENSION_MASK) > 0;
        let cc = (b0 & CC_MASK) as usize;

        let b1 = reader.read_u8()?;
        let marker = (b1 >> MARKER_SHIFT & MARKER_MASK) > 0;
        let payload_type = b1 & PT_MASK;

        let sequence_number = reader.read_u16::<BigEndian>()?;
        let timestamp = reader.read_u32::<BigEndian>()?;
        let ssrc = reader.read_u32::<BigEndian>()?;

        let mut payload_offset = CSRC_OFFSET + (cc * CSRC_LENGTH);
        let mut csrc = vec![];
        for _i in 0..cc {
            csrc.push(reader.read_u32::<BigEndian>()?);
        }

        let (extension_profile, extensions) = if extension {
            let extension_profile = reader.read_u16::<BigEndian>()?;
            payload_offset += 2;
            let extension_length = reader.read_u16::<BigEndian>()? as usize * 4;
            payload_offset += 2;

            let mut payload = vec![0; extension_length];
            reader.read_exact(&mut payload)?;
            payload_offset += payload.len();

            let mut extensions = vec![];
            match extension_profile.into() {
                // RFC 8285 RTP One Byte Header Extension
                ExtensionProfile::OneByte => {
                    let mut curr_offset = 0;
                    while curr_offset < extension_length {
                        if payload[curr_offset] == 0x00 {
                            // padding
                            curr_offset += 1;
                            continue;
                        }

                        let extid = payload[curr_offset] >> 4;
                        let len = ((payload[curr_offset] & (0xFF ^ 0xF0)) + 1) as usize;
                        curr_offset += 1;

                        if extid == EXTENSION_ID_RESERVED {
                            break;
                        }

                        extensions.push(Extension {
                            id: extid,
                            payload: payload[curr_offset..curr_offset + len].to_vec(),
                        });
                        curr_offset += len;
                    }
                }
                // RFC 8285 RTP Two Byte Header Extension
                ExtensionProfile::TwoByte => {
                    let mut curr_offset = 0;
                    while curr_offset < extension_length {
                        if payload[curr_offset] == 0x00 {
                            // padding
                            curr_offset += 1;
                            continue;
                        }

                        let extid = payload[curr_offset];
                        curr_offset += 1;

                        let len = payload[curr_offset] as usize;
                        curr_offset += 1;

                        extensions.push(Extension {
                            id: extid,
                            payload: payload[curr_offset..curr_offset + len].to_vec(),
                        });
                        curr_offset += len;
                    }
                }
                _ => {
                    extensions.push(Extension { id: 0, payload });
                }
            };

            (ExtensionProfile::from(extension_profile), extensions)
        } else {
            (ExtensionProfile::Default, vec![])
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
            payload_offset,
        })
    }

    /// Marshal serializes the packet into bytes.
    pub fn marshal(&self) -> Result<Vec<u8>, Error> {
        todo!()
    }

    /// Serializes the header and writes to the buffer.
    pub fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize, Error> {
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

        // The second byte contains the marker bit and payload type.
        let mut b1 = self.payload_type;
        if self.marker {
            b1 |= 1 << MARKER_SHIFT;
        }
        writer.write_u8(b1)?;

        writer.write_u16::<BigEndian>(self.sequence_number)?;
        writer.write_u32::<BigEndian>(self.timestamp)?;
        writer.write_u32::<BigEndian>(self.ssrc)?;

        for csrc in &self.csrc {
            writer.write_u32::<BigEndian>(*csrc)?;
        }

        if self.extension {
            writer.write_u16::<BigEndian>(self.extension_profile)?;

            let extension_payload_len = self.get_extension_payload_len();
            if self.extension_profile != EXTENSION_PROFILE_ONE_BYTE
                && self.extension_profile != EXTENSION_PROFILE_TWO_BYTE
                && extension_payload_len % 4 != 0
            {
                //the payload must be in 32-bit words.
                return Err(Error::new(
                    "extension_payload must be in 32-bit words".to_string(),
                ));
            }
            let extension_payload_size = (extension_payload_len as u16 + 3) / 4;
            writer.write_u16::<BigEndian>(extension_payload_size)?;

            match self.extension_profile {
                EXTENSION_PROFILE_ONE_BYTE => {
                    for extension in &self.extensions {
                        writer
                            .write_u8((extension.id << 4) | (extension.payload.len() as u8 - 1))?;
                        writer.write_all(&extension.payload)?;
                    }
                }
                EXTENSION_PROFILE_TWO_BYTE => {
                    for extension in &self.extensions {
                        writer.write_u8(extension.id)?;
                        writer.write_u8(extension.payload.len() as u8)?;
                        writer.write_all(&extension.payload)?;
                    }
                }
                _ => {
                    for extension in &self.extensions {
                        writer.write_all(&extension.payload)?;
                    }
                }
            };

            for _ in extension_payload_len..extension_payload_size as usize * 4 {
                writer.write_u8(0)?;
            }
        }

        Ok(writer.flush()?)
    }
}
