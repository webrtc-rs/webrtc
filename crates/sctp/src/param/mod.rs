mod param_chunk_list;
mod param_header;
mod param_type;

use crate::error::Error;

use bytes::{Bytes, BytesMut};

pub(crate) trait Param {
    fn unmarshal(raw: &Bytes) -> Result<Self, Error>
    where
        Self: Sized;
    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error>;
    fn length(&self) -> usize;

    fn marshal(&self) -> Result<Bytes, Error> {
        let mut buf = BytesMut::with_capacity(self.length());
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
