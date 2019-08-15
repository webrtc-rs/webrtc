use std::fmt;
use std::io;

use url::Url;
use utils::Error;

use super::direction::*;
use super::util::*;

#[cfg(test)]
mod extmap_test;

//ExtMap represents the activation of a single RTP header extension
#[derive(Debug, Clone)]
struct ExtMap {
    value: i32,
    direction: Direction,
    uri: Option<Url>,
    ext_attr: Option<String>,
}

impl fmt::Display for ExtMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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

//Clone converts this object to an Attribute
//TODO:
//func (e *ExtMap) Clone() Attribute {
//    return Attribute{Key: "extmap", Value: e.string()}
//}

impl ExtMap {
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
        let value = valdir[0].parse::<u64>()? as i32;
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
        ATTRIBUTE_KEY.to_owned() + "extmap:" + self.to_string().as_str() + END_LINE
    }
}
