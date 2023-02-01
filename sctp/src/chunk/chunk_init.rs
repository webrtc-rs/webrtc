use super::{chunk_header::*, chunk_type::*, *};
use crate::param::param_supported_extensions::ParamSupportedExtensions;
use crate::param::{param_header::*, *};
use crate::util::get_padding_size;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt;

///chunkInitCommon represents an SCTP Chunk body of type INIT and INIT ACK
///
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Type = 1    |  Chunk Flags  |      Chunk Length             |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                         Initiate Tag                          |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|           Advertised Receiver Window Credit (a_rwnd)          |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|  Number of Outbound Streams   |  Number of Inbound Streams    |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                          Initial TSN                          |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|              Optional/Variable-Length Parameters              |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
///The INIT chunk contains the following parameters.  Unless otherwise
///noted, each parameter MUST only be included once in the INIT chunk.
///
///Fixed Parameters                     Status
///----------------------------------------------
///Initiate Tag                        Mandatory
///Advertised Receiver Window Credit   Mandatory
///Number of Outbound Streams          Mandatory
///Number of Inbound Streams           Mandatory
///Initial TSN                         Mandatory
///
///Init represents an SCTP Chunk of type INIT
///
///See chunkInitCommon for the fixed headers
///
///Variable Parameters                  Status     Type Value
///-------------------------------------------------------------
///IPv4 IP (Note 1)               Optional    5
///IPv6 IP (Note 1)               Optional    6
///Cookie Preservative                 Optional    9
///Reserved for ECN Capable (Note 2)   Optional    32768 (0x8000)
///Host Name IP (Note 3)          Optional    11
///Supported IP Types (Note 4)    Optional    12
///
///
/// chunkInitAck represents an SCTP Chunk of type INIT ACK
///
///See chunkInitCommon for the fixed headers
///
///Variable Parameters                  Status     Type Value
///-------------------------------------------------------------
///State Cookie                        Mandatory   7
///IPv4 IP (Note 1)               Optional    5
///IPv6 IP (Note 1)               Optional    6
///Unrecognized Parameter              Optional    8
///Reserved for ECN Capable (Note 2)   Optional    32768 (0x8000)
///Host Name IP (Note 3)          Optional    11<Paste>
#[derive(Default, Debug)]
pub(crate) struct ChunkInit {
    pub(crate) is_ack: bool,
    pub(crate) initiate_tag: u32,
    pub(crate) advertised_receiver_window_credit: u32,
    pub(crate) num_outbound_streams: u16,
    pub(crate) num_inbound_streams: u16,
    pub(crate) initial_tsn: u32,
    pub(crate) params: Vec<Box<dyn Param + Send + Sync>>,
}

impl Clone for ChunkInit {
    fn clone(&self) -> Self {
        ChunkInit {
            is_ack: self.is_ack,
            initiate_tag: self.initiate_tag,
            advertised_receiver_window_credit: self.advertised_receiver_window_credit,
            num_outbound_streams: self.num_outbound_streams,
            num_inbound_streams: self.num_inbound_streams,
            initial_tsn: self.initial_tsn,
            params: self.params.to_vec(),
        }
    }
}

pub(crate) const INIT_CHUNK_MIN_LENGTH: usize = 16;
pub(crate) const INIT_OPTIONAL_VAR_HEADER_LENGTH: usize = 4;

/// makes chunkInitCommon printable
impl fmt::Display for ChunkInit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = format!(
            "is_ack: {}
            initiate_tag: {}
            advertised_receiver_window_credit: {}
            num_outbound_streams: {}
            num_inbound_streams: {}
            initial_tsn: {}",
            self.is_ack,
            self.initiate_tag,
            self.advertised_receiver_window_credit,
            self.num_outbound_streams,
            self.num_inbound_streams,
            self.initial_tsn,
        );

        for (i, param) in self.params.iter().enumerate() {
            res += format!("Param {i}:\n {param}").as_str();
        }
        write!(f, "{} {}", self.header(), res)
    }
}

impl Chunk for ChunkInit {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: if self.is_ack { CT_INIT_ACK } else { CT_INIT },
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    ///https://tools.ietf.org/html/rfc4960#section-3.2.1
    ///
    ///Chunk values of SCTP control chunks consist of a chunk-type-specific
    ///header of required fields, followed by zero or more parameters.  The
    ///optional and variable-length parameters contained in a chunk are
    ///defined in a Type-Length-Value format as shown below.
    ///
    ///0                   1                   2                   3
    ///0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    ///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///|          Parameter Type       |       Parameter Length        |
    ///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    ///|                                                               |
    ///|                       Parameter Value                         |
    ///|                                                               |
    ///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ChunkHeader::unmarshal(raw)?;

        if !(header.typ == CT_INIT || header.typ == CT_INIT_ACK) {
            return Err(Error::ErrChunkTypeNotTypeInit);
        } else if header.value_length() < INIT_CHUNK_MIN_LENGTH {
            // validity of value_length is checked in ChunkHeader::unmarshal
            return Err(Error::ErrChunkValueNotLongEnough);
        }

        // The Chunk Flags field in INIT is reserved, and all bits in it should
        // be set to 0 by the sender and ignored by the receiver.  The sequence
        // of parameters within an INIT can be processed in any order.
        if header.flags != 0 {
            return Err(Error::ErrChunkTypeInitFlagZero);
        }

        let reader = &mut raw.slice(CHUNK_HEADER_SIZE..CHUNK_HEADER_SIZE + header.value_length());

        let initiate_tag = reader.get_u32();
        let advertised_receiver_window_credit = reader.get_u32();
        let num_outbound_streams = reader.get_u16();
        let num_inbound_streams = reader.get_u16();
        let initial_tsn = reader.get_u32();

        let mut params = vec![];
        let mut offset = CHUNK_HEADER_SIZE + INIT_CHUNK_MIN_LENGTH;
        let mut remaining = raw.len() as isize - offset as isize;
        while remaining > INIT_OPTIONAL_VAR_HEADER_LENGTH as isize {
            let p = build_param(&raw.slice(offset..CHUNK_HEADER_SIZE + header.value_length()))?;
            let p_len = PARAM_HEADER_LENGTH + p.value_length();
            let len_plus_padding = p_len + get_padding_size(p_len);
            params.push(p);
            offset += len_plus_padding;
            remaining -= len_plus_padding as isize;
        }

        Ok(ChunkInit {
            is_ack: header.typ == CT_INIT_ACK,
            initiate_tag,
            advertised_receiver_window_credit,
            num_outbound_streams,
            num_inbound_streams,
            initial_tsn,
            params,
        })
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(writer)?;

        writer.put_u32(self.initiate_tag);
        writer.put_u32(self.advertised_receiver_window_credit);
        writer.put_u16(self.num_outbound_streams);
        writer.put_u16(self.num_inbound_streams);
        writer.put_u32(self.initial_tsn);
        for (idx, p) in self.params.iter().enumerate() {
            let pp = p.marshal()?;
            let pp_len = pp.len();
            writer.extend(pp);

            // Chunks (including Type, Length, and Value fields) are padded out
            // by the sender with all zero bytes to be a multiple of 4 bytes
            // long.  This padding MUST NOT be more than 3 bytes in total.  The
            // Chunk Length value does not include terminating padding of the
            // chunk.  *However, it does include padding of any variable-length
            // parameter except the last parameter in the chunk.*  The receiver
            // MUST ignore the padding.
            if idx != self.params.len() - 1 {
                let cnt = get_padding_size(pp_len);
                writer.extend(vec![0u8; cnt]);
            }
        }

        Ok(writer.len())
    }

    fn check(&self) -> Result<()> {
        // The receiver of the INIT (the responding end) records the value of
        // the Initiate Tag parameter.  This value MUST be placed into the
        // Verification Tag field of every SCTP packet that the receiver of
        // the INIT transmits within this association.
        //
        // The Initiate Tag is allowed to have any value except 0.  See
        // Section 5.3.1 for more on the selection of the tag value.
        //
        // If the value of the Initiate Tag in a received INIT chunk is found
        // to be 0, the receiver MUST treat it as an error and close the
        // association by transmitting an ABORT.
        if self.initiate_tag == 0 {
            return Err(Error::ErrChunkTypeInitInitateTagZero);
        }

        // Defines the maximum number of streams the sender of this INIT
        // chunk allows the peer end to create in this association.  The
        // value 0 MUST NOT be used.
        //
        // Note: There is no negotiation of the actual number of streams but
        // instead the two endpoints will use the min(requested, offered).
        // See Section 5.1.1 for details.
        //
        // Note: A receiver of an INIT with the MIS value of 0 SHOULD abort
        // the association.
        if self.num_inbound_streams == 0 {
            return Err(Error::ErrInitInboundStreamRequestZero);
        }

        // Defines the number of outbound streams the sender of this INIT
        // chunk wishes to create in this association.  The value of 0 MUST
        // NOT be used.
        //
        // Note: A receiver of an INIT with the OS value set to 0 SHOULD
        // abort the association.

        if self.num_outbound_streams == 0 {
            return Err(Error::ErrInitOutboundStreamRequestZero);
        }

        // An SCTP receiver MUST be able to receive a minimum of 1500 bytes in
        // one SCTP packet.  This means that an SCTP endpoint MUST NOT indicate
        // less than 1500 bytes in its initial a_rwnd sent in the INIT or INIT
        // ACK.
        if self.advertised_receiver_window_credit < 1500 {
            return Err(Error::ErrInitAdvertisedReceiver1500);
        }

        Ok(())
    }

    fn value_length(&self) -> usize {
        let mut l = 4 + 4 + 2 + 2 + 4;
        for (idx, p) in self.params.iter().enumerate() {
            let p_len = PARAM_HEADER_LENGTH + p.value_length();
            l += p_len;
            if idx != self.params.len() - 1 {
                l += get_padding_size(p_len);
            }
        }
        l
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

impl ChunkInit {
    pub(crate) fn set_supported_extensions(&mut self) {
        // TODO RFC5061 https://tools.ietf.org/html/rfc6525#section-5.2
        // An implementation supporting this (Supported Extensions Parameter)
        // extension MUST list the ASCONF, the ASCONF-ACK, and the AUTH chunks
        // in its INIT and INIT-ACK parameters.
        self.params.push(Box::new(ParamSupportedExtensions {
            chunk_types: vec![CT_RECONFIG, CT_FORWARD_TSN],
        }));
    }
}
