use super::*;
use crate::param::*;

use crate::param::param_header::PARAM_HEADER_LENGTH;
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
pub(crate) struct ChunkInitCommon {
    pub(crate) initiate_tag: u32,
    pub(crate) advertised_receiver_window_credit: u32,
    pub(crate) num_outbound_streams: u16,
    pub(crate) num_inbound_streams: u16,
    pub(crate) initial_tsn: u32,
    pub(crate) params: Vec<Box<dyn Param>>,
}

pub(crate) const INIT_CHUNK_MIN_LENGTH: usize = 16;
pub(crate) const INIT_OPTIONAL_VAR_HEADER_LENGTH: usize = 4;

/// makes chunkInitCommon printable
impl fmt::Display for ChunkInitCommon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = format!(
            "initiate_tag: {}
		advertised_receiver_window_credit: {}
		num_outbound_streams: {}
		num_inbound_streams: {}
		initial_tsn: {}",
            self.initiate_tag,
            self.advertised_receiver_window_credit,
            self.num_outbound_streams,
            self.num_inbound_streams,
            self.initial_tsn,
        );

        for (i, param) in self.params.iter().enumerate() {
            res += format!("Param {}:\n {}", i, param).as_str();
        }
        write!(f, "{}", res)
    }
}

impl ChunkInitCommon {
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
    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        if raw.len() < INIT_CHUNK_MIN_LENGTH {
            return Err(Error::ErrChunkTooShort);
        }

        let reader = &mut raw.clone();

        let initiate_tag = reader.get_u32();
        let advertised_receiver_window_credit = reader.get_u32();
        let num_outbound_streams = reader.get_u16();
        let num_inbound_streams = reader.get_u16();
        let initial_tsn = reader.get_u32();

        let mut params = vec![];
        let mut offset = INIT_CHUNK_MIN_LENGTH;
        let mut remaining = raw.len() - offset;
        while remaining > INIT_OPTIONAL_VAR_HEADER_LENGTH {
            let p = build_param(&raw.slice(offset..))?;
            let p_len = PARAM_HEADER_LENGTH + p.value_length();
            let len_plus_padding = p_len + get_padding_size(p_len);
            params.push(p);
            offset += len_plus_padding;
            remaining -= len_plus_padding;
        }

        Ok(ChunkInitCommon {
            initiate_tag,
            advertised_receiver_window_credit,
            num_outbound_streams,
            num_inbound_streams,
            initial_tsn,
            params,
        })
    }

    fn marshal(&self) -> Result<Bytes, Error> {
        let mut writer = BytesMut::with_capacity(INIT_CHUNK_MIN_LENGTH);

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

        Ok(writer.freeze())
    }
}
