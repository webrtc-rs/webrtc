#[cfg(test)]
mod h264_test;

use super::*;

fn profile_level_id_matches(a: &str, b: &str) -> bool {
    let aa = match hex::decode(a) {
        Ok(aa) => {
            if aa.len() < 2 {
                return false;
            }
            aa
        }
        Err(_) => return false,
    };

    let bb = match hex::decode(b) {
        Ok(bb) => {
            if bb.len() < 2 {
                return false;
            }
            bb
        }
        Err(_) => return false,
    };

    aa[0] == bb[0] && aa[1] == bb[1]
}

#[derive(Debug, PartialEq)]
pub(crate) struct H264Fmtp {
    pub(crate) parameters: HashMap<String, String>,
}

impl Fmtp for H264Fmtp {
    fn mime_type(&self) -> &str {
        "video/h264"
    }

    /// Match returns true if h and b are compatible fmtp descriptions
    /// Based on RFC6184 Section 8.2.2:
    ///   The parameters identifying a media format configuration for H.264
    ///   are profile-level-id and packetization-mode.  These media format
    ///   configuration parameters (except for the level part of profile-
    ///   level-id) MUST be used symmetrically; that is, the answerer MUST
    ///   either maintain all configuration parameters or remove the media
    ///   format (payload type) completely if one or more of the parameter
    ///   values are not supported.
    ///     Informative note: The requirement for symmetric use does not
    ///     apply for the level part of profile-level-id and does not apply
    ///     for the other stream properties and capability parameters.
    fn match_fmtp(&self, f: &(dyn Fmtp)) -> bool {
        if let Some(c) = f.as_any().downcast_ref::<H264Fmtp>() {
            // test packetization-mode
            let hpmode = match self.parameters.get("packetization-mode") {
                Some(s) => s,
                None => return false,
            };
            let cpmode = match c.parameters.get("packetization-mode") {
                Some(s) => s,
                None => return false,
            };

            if hpmode != cpmode {
                return false;
            }

            // test profile-level-id
            let hplid = match self.parameters.get("profile-level-id") {
                Some(s) => s,
                None => return false,
            };
            let cplid = match c.parameters.get("profile-level-id") {
                Some(s) => s,
                None => return false,
            };

            if !profile_level_id_matches(hplid, cplid) {
                return false;
            }

            true
        } else {
            false
        }
    }

    fn parameter(&self, key: &str) -> Option<&String> {
        self.parameters.get(key)
    }

    fn equal(&self, other: &(dyn Fmtp)) -> bool {
        other
            .as_any()
            .downcast_ref::<H264Fmtp>()
            .map_or(false, |a| self == a)
    }

    fn as_any(&self) -> &(dyn Any) {
        self
    }
}
