use std::fmt;
use std::io::Cursor;

use utils::Error;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

// Header represents an RTP packet header
// NOTE: PayloadOffset is populated by Marshal/Unmarshal and should not be modified
// NOTE: Raw is populated by Marshal/Unmarshal and should not be modified

// Packet represents an RTP Packet
#[derive(Debug)]
struct Packet {
    version: u8,
    padding: bool,
    extension: bool,
    marker: bool,
    payload_type: u8,
    sequence_number: u16,
    timestamp: u32,
    ssrc: u32,
    csrc: Vec<u32>,
    extension_profile: u16,
    extension_payload: Vec<u8>,
    payload: Vec<u8>,
}

const HEADER_LENGTH: usize = 4;
const VERSION_SHIFT: u8 = 6;
const VERSION_MASK: u8 = 0x3;
const PADDING_SHIFT: u8 = 5;
const PADDING_MASK: u8 = 0x1;
const EXTENSION_SHIFT: u8 = 4;
const EXTENSION_MASK: u8 = 0x1;
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

impl fmt::Display for Packet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut out = "RTP PACKET:\n".to_string();

        out += format!("\tVersion: {}\n", self.version).as_str();
        out += format!("\tMarker: {}\n", self.marker).as_str();
        out += format!("\tPayload Type: {}\n", self.payload_type).as_str();
        out += format!("\tSequence Number: {}\n", self.sequence_number).as_str();
        out += format!("\tTimestamp: {}\n", self.timestamp).as_str();
        out += format!("\tSSRC: {} ({:x})\n", self.ssrc, self.ssrc).as_str();
        out += format!("\tPayload Length: {}\n", self.payload.len()).as_str();

        write!(f, "{}", out)
    }
}

impl Packet {
    // Unmarshal parses the passed byte slice and stores the result in the Header this method is called upon
    pub fn unmarshal(raw_packet: &[u8]) -> Result<Self, Error> {
        if raw_packet.len() < HEADER_LENGTH {
            return Err(Error::new(format!(
                "RTP header size insufficient; {} < {}",
                raw_packet.len(),
                HEADER_LENGTH
            )));
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

        let version = raw_packet[0] >> VERSION_SHIFT & VERSION_MASK;
        let padding = (raw_packet[0] >> PADDING_SHIFT & PADDING_MASK) > 0;
        let extension = (raw_packet[0] >> EXTENSION_SHIFT & EXTENSION_MASK) > 0;
        let cc = (raw_packet[0] & CC_MASK) as usize;

        let marker = (raw_packet[1] >> MARKER_SHIFT & MARKER_MASK) > 0;
        let payload_type = raw_packet[1] & PT_MASK;

        let mut rdr = Cursor::new(&raw_packet[SEQ_NUM_OFFSET..]);
        let sequence_number = rdr.read_u16::<BigEndian>()?;
        let timestamp = rdr.read_u32::<BigEndian>()?;
        let ssrc = rdr.read_u32::<BigEndian>()?;

        let mut curr_offset = CSRC_OFFSET + (cc * CSRC_LENGTH);
        if raw_packet.len() < curr_offset {
            return Err(Error::new(format!(
                "RTP header size insufficient; {} < {}",
                raw_packet.len(),
                curr_offset
            )));
        }

        let mut csrc = vec![];
        for i in 0..cc {
            let offset = CSRC_OFFSET + (i * CSRC_LENGTH);
            csrc.push(rdr.read_u32::<BigEndian>()?);
        }

        let (extension_profile, extension_payload) = if extension {
            if raw_packet.len() < curr_offset + 4 {
                return Err(Error::new(format!(
                    "RTP header size insufficient for extension; {} < {}",
                    raw_packet.len(),
                    curr_offset + 4
                )));
            }

            let extension_profile = rdr.read_u16::<BigEndian>()?;
            curr_offset += 2;
            let extension_length = rdr.read_u16::<BigEndian>()? as usize * 4;
            curr_offset += 2;

            if raw_packet.len() < curr_offset + extension_length {
                return Err(Error::new(format!(
                    "RTP header size insufficient for extension length; {} < {}",
                    raw_packet.len(),
                    curr_offset + extension_length
                )));
            }

            let extension_payload = &raw_packet[curr_offset..curr_offset + extension_length];
            curr_offset += extension_payload.len();
            (extension_profile, extension_payload.to_vec())
        } else {
            (0, vec![])
        };

        let payload = (&raw_packet[curr_offset..]).to_vec();

        Ok(Packet {
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
            extension_payload,
            payload,
        })
    }

    // MarshalSize returns the size of the packet once marshaled.
    pub fn marshal_size(&self) -> usize {
        let mut head_size = 12 + (self.csrc.len() * CSRC_LENGTH);
        if self.extension {
            head_size += 4 + self.extension_payload.len();
        }
        head_size + self.payload.len()
    }

    // Marshal serializes the header and writes to the buffer.
    pub fn marshal(&self) -> Result<Vec<u8>, Error> {
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
        let marshal_size = self.marshal_size();
        let mut buf = Vec::with_capacity(marshal_size);

        // The first byte contains the version, padding bit, extension bit, and csrc size
        let mut b0 = (self.version << VERSION_SHIFT) | self.csrc.len() as u8;
        if self.padding {
            b0 |= 1 << PADDING_SHIFT;
        }

        if self.extension {
            b0 |= 1 << EXTENSION_SHIFT;
        }
        buf.push(b0);

        // The second byte contains the marker bit and payload type.
        let mut b1 = self.payload_type;
        if self.marker {
            b1 |= 1 << MARKER_SHIFT;
        }
        buf.push(b1);

        buf.write_u16::<BigEndian>(self.sequence_number)?;
        buf.write_u32::<BigEndian>(self.timestamp)?;
        buf.write_u32::<BigEndian>(self.ssrc)?;

        let mut curr_offset = CSRC_OFFSET;
        for csrc in &self.csrc {
            buf.write_u32::<BigEndian>(*csrc)?;
            curr_offset += 4;
        }

        if self.extension {
            if self.extension_payload.len() % 4 != 0 {
                //the payload must be in 32-bit words.
                return Err(Error::new(
                    "extension_payload must be in 32-bit words".to_string(),
                ));
            }
            let extension_payload_size = self.extension_payload.len();
            buf.write_u16::<BigEndian>(self.extension_profile)?;
            buf.write_u16::<BigEndian>((extension_payload_size / 4) as u16)?;
            curr_offset += 4;

            {
                let (left, right) = buf.split_at_mut(curr_offset);
                {
                    let (extension_payload, others) =
                        right.split_at_mut(self.extension_payload.len());
                    extension_payload.clone_from_slice(&self.extension_payload);
                }
            }
            curr_offset += self.extension_payload.len();
        }

        {
            let (header, payload) = buf.split_at_mut(curr_offset);
            payload.clone_from_slice(&self.payload);
        }

        Ok(buf)
    }
}
