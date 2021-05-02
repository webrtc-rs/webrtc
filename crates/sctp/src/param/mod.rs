pub(crate) mod param_chunk_list;
pub(crate) mod param_forward_tsn_supported;
pub(crate) mod param_header;
pub(crate) mod param_heartbeat_info;
pub(crate) mod param_outgoing_reset_request;
pub(crate) mod param_type;

use crate::error::Error;
use param_header::*;

use bytes::{Bytes, BytesMut};

pub(crate) trait Param {
    fn unmarshal(raw: &Bytes) -> Result<Self, Error>
    where
        Self: Sized;
    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error>;
    fn value_length(&self) -> usize;

    fn marshal(&self) -> Result<Bytes, Error> {
        let capacity = PARAM_HEADER_LENGTH + self.value_length();
        let mut buf = BytesMut::with_capacity(capacity);
        self.marshal_to(&mut buf)?;
        Ok(buf.freeze())
    }
}

/*TODO:
func buildParam(t paramType, rawParam []byte) (param, error) {
    switch t {
    case FORWARD_TSNSUPP:
        return (&paramForwardTSNSupported{}).unmarshal(rawParam)
    case SUPPORTED_EXT:
        return (&paramSupportedExtensions{}).unmarshal(rawParam)
    case RANDOM:
        return (&paramRandom{}).unmarshal(rawParam)
    case REQ_HMACALGO:
        return (&paramRequestedHMACAlgorithm{}).unmarshal(rawParam)
    case CHUNK_LIST:
        return (&paramChunkList{}).unmarshal(rawParam)
    case STATE_COOKIE:
        return (&paramStateCookie{}).unmarshal(rawParam)
    case HEARTBEAT_INFO:
        return (&paramHeartbeatInfo{}).unmarshal(rawParam)
    case OUT_SSNRESET_REQ:
        return (&paramOutgoingResetRequest{}).unmarshal(rawParam)
    case RECONFIG_RESP:
        return (&paramReconfigResponse{}).unmarshal(rawParam)
    default:
        return nil, fmt.Errorf("%w: %v", errParamTypeUnhandled, t)
    }
}
 */
