use super::sdp_type::SDPType;
use crate::error::Error;

use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// SessionDescription is used to expose local and remote session descriptions.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SessionDescription {
    pub typ: SDPType,
    pub sdp: String,
    // This will never be initialized by callers, internal use only
    //parsed *sdp.SessionDescription
}

/// Unmarshal is a helper to deserialize the sdp
impl SessionDescription {
    pub fn unmarshal(&self) -> Result<sdp::session_description::SessionDescription, Error> {
        let mut reader = Cursor::new(self.sdp.as_bytes());
        let parsed = sdp::session_description::SessionDescription::unmarshal(&mut reader)?;
        Ok(parsed)
    }
}
