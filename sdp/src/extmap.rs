use std::fmt;
use std::io;

use url::Url;
use util::Error;

use super::common_description::*;
use super::direction::*;

#[cfg(test)]
mod extmap_test;

// Default ext values
pub const DEF_EXT_MAP_VALUE_ABS_SEND_TIME: usize = 1;
pub const DEF_EXT_MAP_VALUE_TRANSPORT_CC: usize = 2;
pub const DEF_EXT_MAP_VALUE_SDES_MID: usize = 3;
pub const DEF_EXT_MAP_VALUE_SDES_RTP_STREAM_ID: usize = 4;

pub const ABS_SEND_TIME_URI: &'static str =
    "http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time";
pub const TRANSPORT_CC_URI: &'static str =
    "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01";
pub const SDES_MID_URI: &'static str = "urn:ietf:params:rtp-hdrext:sdes:mid";
pub const SDES_RTP_STREAM_ID_URI: &'static str = "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id";

//ExtMap represents the activation of a single RTP header extension
#[derive(Debug, Clone, Default)]
struct ExtMap {
    value: isize,
    direction: Direction,
    uri: Option<Url>,
    ext_attr: Option<String>,
}

impl fmt::Display for ExtMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut output = format!("{}", self.value);
        let dirstring = self.direction.to_string();
        if &dirstring != DIRECTION_UNKNOWN_STR {
            output += format!("/{}", dirstring).as_str();
        }

        if let Some(uri) = &self.uri {
            output += format!(" {}", uri).as_str();
        }

        if let Some(ext_attr) = &self.ext_attr {
            output += format!(" {}", ext_attr).as_str();
        }

        write!(f, "{}", output)
    }
}

impl ExtMap {
    //Clone converts this object to an Attribute
    pub fn convert(&self) -> Attribute {
        return Attribute {
            key: "extmap".to_string(),
            value: Some(self.to_string()),
        };
    }

    //Unmarshal creates an Extmap from a string
    pub fn unmarshal<R: io::BufRead>(reader: &mut R) -> Result<Self, Error> {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let parts: Vec<&str> = line.trim().splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(Error::new(format!("SyntaxError: {}", line)));
        }

        let fields: Vec<&str> = parts[1].split_whitespace().collect();
        if fields.len() < 2 {
            return Err(Error::new(format!("SyntaxError: {}", line)));
        }

        let valdir: Vec<&str> = fields[0].split("/").collect();
        let value = valdir[0].parse::<isize>()?;
        if value < 1 || value > 246 {
            return Err(Error::new(format!(
                "SyntaxError: {} -- extmap key must be in the range 1-256",
                valdir[0]
            )));
        }

        let mut direction = Direction::DirectionUnknown;
        if valdir.len() == 2 {
            direction = Direction::new(valdir[1]);
            if direction == Direction::DirectionUnknown {
                return Err(Error::new(format!("unknown direction from {}", valdir[1])));
            }
        }

        let uri = Some(Url::parse(fields[1])?);

        let ext_attr = if fields.len() == 3 {
            Some(fields[2].to_owned())
        } else {
            None
        };

        Ok(ExtMap {
            value,
            direction,
            uri,
            ext_attr,
        })
    }

    //Marshal creates a string from an ExtMap
    pub fn marshal(&self) -> String {
        "extmap:".to_string() + self.to_string().as_str()
    }
}
