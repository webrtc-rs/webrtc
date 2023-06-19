use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::param_header::*;
use super::param_type::*;
use super::*;

pub(crate) const PARAM_OUTGOING_RESET_REQUEST_STREAM_IDENTIFIERS_OFFSET: usize = 12;

///This parameter is used by the sender to request the reset of some or
///all outgoing streams.
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|     Parameter Type = 13       | Parameter Length = 16 + 2 * N |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|           Re-configuration Request Sequence Number            |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|           Re-configuration Response Sequence Number           |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                Sender's Last Assigned TSN                     |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|  Stream Number 1 (optional)   |    Stream Number 2 (optional) |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                            ......                             |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|  Stream Number N-1 (optional) |    Stream Number N (optional) |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct ParamOutgoingResetRequest {
    /// reconfig_request_sequence_number is used to identify the request.  It is a monotonically
    /// increasing number that is initialized to the same value as the
    /// initial TSN.  It is increased by 1 whenever sending a new Re-
    /// configuration Request Parameter.
    pub(crate) reconfig_request_sequence_number: u32,
    /// When this Outgoing SSN Reset Request Parameter is sent in response
    /// to an Incoming SSN Reset Request Parameter, this parameter is also
    /// an implicit response to the incoming request.  This field then
    /// holds the Re-configuration Request Sequence Number of the incoming
    /// request.  In other cases, it holds the next expected
    /// Re-configuration Request Sequence Number minus 1.
    pub(crate) reconfig_response_sequence_number: u32,
    /// This value holds the next TSN minus 1 -- in other words, the last
    /// TSN that this sender assigned.
    pub(crate) sender_last_tsn: u32,
    /// This optional field, if included, is used to indicate specific
    /// streams that are to be reset.  If no streams are listed, then all
    /// streams are to be reset.
    pub(crate) stream_identifiers: Vec<u16>,
}

impl fmt::Display for ParamOutgoingResetRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {} {:?}",
            self.header(),
            self.reconfig_request_sequence_number,
            self.reconfig_request_sequence_number,
            self.reconfig_response_sequence_number,
            self.stream_identifiers
        )
    }
}

impl Param for ParamOutgoingResetRequest {
    fn header(&self) -> ParamHeader {
        ParamHeader {
            typ: ParamType::OutSsnResetReq,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ParamHeader::unmarshal(raw)?;

        // validity of value_length is checked in ParamHeader::unmarshal
        if header.value_length() < PARAM_OUTGOING_RESET_REQUEST_STREAM_IDENTIFIERS_OFFSET {
            return Err(Error::ErrSsnResetRequestParamTooShort);
        }

        let reader =
            &mut raw.slice(PARAM_HEADER_LENGTH..PARAM_HEADER_LENGTH + header.value_length());
        let reconfig_request_sequence_number = reader.get_u32();
        let reconfig_response_sequence_number = reader.get_u32();
        let sender_last_tsn = reader.get_u32();

        let lim =
            (header.value_length() - PARAM_OUTGOING_RESET_REQUEST_STREAM_IDENTIFIERS_OFFSET) / 2;
        let mut stream_identifiers = vec![];
        for _ in 0..lim {
            stream_identifiers.push(reader.get_u16());
        }

        Ok(ParamOutgoingResetRequest {
            reconfig_request_sequence_number,
            reconfig_response_sequence_number,
            sender_last_tsn,
            stream_identifiers,
        })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(buf)?;
        buf.put_u32(self.reconfig_request_sequence_number);
        buf.put_u32(self.reconfig_response_sequence_number);
        buf.put_u32(self.sender_last_tsn);
        for sid in &self.stream_identifiers {
            buf.put_u16(*sid);
        }
        Ok(buf.len())
    }

    fn value_length(&self) -> usize {
        PARAM_OUTGOING_RESET_REQUEST_STREAM_IDENTIFIERS_OFFSET + self.stream_identifiers.len() * 2
    }

    fn clone_to(&self) -> Box<dyn Param + Send + Sync> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
