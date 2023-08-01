#[cfg(test)]
mod param_test;

pub(crate) mod param_chunk_list;
pub(crate) mod param_forward_tsn_supported;
pub(crate) mod param_header;
pub(crate) mod param_heartbeat_info;
pub(crate) mod param_outgoing_reset_request;
pub(crate) mod param_random;
pub(crate) mod param_reconfig_response;
pub(crate) mod param_requested_hmac_algorithm;
pub(crate) mod param_state_cookie;
pub(crate) mod param_supported_extensions;
pub(crate) mod param_type;
pub(crate) mod param_unknown;
pub(crate) mod param_unrecognized;

use std::any::Any;
use std::fmt;

use bytes::{Buf, Bytes, BytesMut};
use param_header::*;
use param_type::*;

use crate::error::{Error, Result};
use crate::param::param_chunk_list::ParamChunkList;
use crate::param::param_forward_tsn_supported::ParamForwardTsnSupported;
use crate::param::param_heartbeat_info::ParamHeartbeatInfo;
use crate::param::param_outgoing_reset_request::ParamOutgoingResetRequest;
use crate::param::param_random::ParamRandom;
use crate::param::param_reconfig_response::ParamReconfigResponse;
use crate::param::param_requested_hmac_algorithm::ParamRequestedHmacAlgorithm;
use crate::param::param_state_cookie::ParamStateCookie;
use crate::param::param_supported_extensions::ParamSupportedExtensions;
use crate::param::param_unknown::ParamUnknown;

pub(crate) trait Param: fmt::Display + fmt::Debug {
    fn header(&self) -> ParamHeader;
    fn unmarshal(raw: &Bytes) -> Result<Self>
    where
        Self: Sized;
    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize>;
    fn value_length(&self) -> usize;
    fn clone_to(&self) -> Box<dyn Param + Send + Sync>;
    fn as_any(&self) -> &(dyn Any + Send + Sync);

    fn marshal(&self) -> Result<Bytes> {
        let capacity = PARAM_HEADER_LENGTH + self.value_length();
        let mut buf = BytesMut::with_capacity(capacity);
        self.marshal_to(&mut buf)?;
        Ok(buf.freeze())
    }
}

impl Clone for Box<dyn Param + Send + Sync> {
    fn clone(&self) -> Box<dyn Param + Send + Sync> {
        self.clone_to()
    }
}

pub(crate) fn build_param(raw_param: &Bytes) -> Result<Box<dyn Param + Send + Sync>> {
    if raw_param.len() < PARAM_HEADER_LENGTH {
        return Err(Error::ErrParamHeaderTooShort);
    }
    let reader = &mut raw_param.slice(..2);
    let raw_type = reader.get_u16();
    match raw_type.into() {
        ParamType::ForwardTsnSupp => Ok(Box::new(ParamForwardTsnSupported::unmarshal(raw_param)?)),
        ParamType::SupportedExt => Ok(Box::new(ParamSupportedExtensions::unmarshal(raw_param)?)),
        ParamType::Random => Ok(Box::new(ParamRandom::unmarshal(raw_param)?)),
        ParamType::ReqHmacAlgo => Ok(Box::new(ParamRequestedHmacAlgorithm::unmarshal(raw_param)?)),
        ParamType::ChunkList => Ok(Box::new(ParamChunkList::unmarshal(raw_param)?)),
        ParamType::StateCookie => Ok(Box::new(ParamStateCookie::unmarshal(raw_param)?)),
        ParamType::HeartbeatInfo => Ok(Box::new(ParamHeartbeatInfo::unmarshal(raw_param)?)),
        ParamType::OutSsnResetReq => Ok(Box::new(ParamOutgoingResetRequest::unmarshal(raw_param)?)),
        ParamType::ReconfigResp => Ok(Box::new(ParamReconfigResponse::unmarshal(raw_param)?)),
        _ => {
            // According to RFC https://datatracker.ietf.org/doc/html/rfc4960#section-3.2.1
            let stop_processing = ((raw_type >> 15) & 0x01) == 0;
            if stop_processing {
                Err(Error::ErrParamTypeUnhandled { typ: raw_type })
            } else {
                // We still might need to report this param as unrecognized.
                // This depends on the context though.
                Ok(Box::new(ParamUnknown::unmarshal(raw_param)?))
            }
        }
    }
}
