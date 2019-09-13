use std::collections::HashMap;
use std::{fmt, io};

use util::Error;

use super::session_description::SessionDescription;

pub const END_LINE: &'static str = "\r\n";
pub const ATTRIBUTE_KEY: &'static str = "a=";

// ConnectionRole indicates which of the end points should initiate the connection establishment
#[derive(Debug)]
pub enum ConnectionRole {
    // ConnectionRoleActive indicates the endpoint will initiate an outgoing connection.
    ConnectionRoleActive = 1,

    // ConnectionRolePassive indicates the endpoint will accept an incoming connection.
    ConnectionRolePassive = 2,

    // ConnectionRoleActpass indicates the endpoint is willing to accept an incoming connection or to initiate an outgoing connection.
    ConnectionRoleActpass = 3,

    // ConnectionRoleHoldconn indicates the endpoint does not want the connection to be established for the time being.
    ConnectionRoleHoldconn = 4,
}

impl fmt::Display for ConnectionRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ConnectionRole::ConnectionRoleActive => "active",
            ConnectionRole::ConnectionRolePassive => "passive",
            ConnectionRole::ConnectionRoleActpass => "actpass",
            ConnectionRole::ConnectionRoleHoldconn => "holdconn",
            //_ => "Unknown",
        };
        write!(f, "{}", s)
    }
}

pub(crate) fn new_session_id() -> u64 {
    rand::random::<u64>()
}

// Codec represents a codec
#[derive(Debug, Clone)]
pub struct Codec {
    payload_type: u8,
    name: String,
    clock_rate: u32,
    encoding_parameters: String,
    fmtp: String,
}

impl fmt::Display for Codec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}/{}/{} ({})",
            self.payload_type, self.name, self.clock_rate, self.encoding_parameters, self.fmtp,
        )
    }
}

pub(crate) fn parse_rtpmap(rtpmap: &str) -> Result<Codec, Error> {
    let parsing_failed = Error::new("could not extract codec from rtpmap".to_string());

    // a=rtpmap:<payload type> <encoding name>/<clock rate>[/<encoding parameters>]
    let split: Vec<&str> = rtpmap.split_whitespace().collect();
    if split.len() != 2 {
        return Err(parsing_failed);
    }

    let pt_split: Vec<&str> = split[0].split(":").collect();
    if pt_split.len() != 2 {
        return Err(parsing_failed);
    }
    let payload_type = pt_split[1].parse::<u8>()?;

    let split: Vec<&str> = split[1].split("/").collect();
    let name = split[0].to_string();
    let parts = split.len();
    let clock_rate = if parts > 1 {
        split[1].parse::<u32>()?
    } else {
        0
    };
    let encoding_parameters = if parts > 2 {
        split[2].to_string()
    } else {
        "".to_string()
    };

    Ok(Codec {
        payload_type,
        name,
        clock_rate,
        encoding_parameters,
        fmtp: "".to_string(),
    })
}

pub(crate) fn parse_fmtp(fmtp: &str) -> Result<Codec, Error> {
    let parsing_failed = Error::new("could not extract codec from fmtp".to_string());

    // a=fmtp:<format> <format specific parameters>
    let split: Vec<&str> = fmtp.split_whitespace().collect();
    if split.len() != 2 {
        return Err(parsing_failed);
    }

    let fmtp = split[1].to_string();

    let split: Vec<&str> = split[0].split(":").collect();
    if split.len() != 2 {
        return Err(parsing_failed);
    }
    let payload_type = split[1].parse::<u8>()?;

    Ok(Codec {
        payload_type,
        name: "".to_string(),
        clock_rate: 0,
        encoding_parameters: "".to_string(),
        fmtp,
    })
}

pub(crate) fn merge_codecs(codec: Codec, codecs: &mut HashMap<u8, Codec>) {
    if let Some(saved_codec) = codecs.get_mut(&codec.payload_type) {
        if saved_codec.payload_type == 0 {
            saved_codec.payload_type = codec.payload_type
        }
        if &saved_codec.name == "" {
            saved_codec.name = codec.name
        }
        if saved_codec.clock_rate == 0 {
            saved_codec.clock_rate = codec.clock_rate
        }
        if &saved_codec.encoding_parameters == "" {
            saved_codec.encoding_parameters = codec.encoding_parameters
        }
        if &saved_codec.fmtp == "" {
            saved_codec.fmtp = codec.fmtp
        }
    } else {
        codecs.insert(codec.payload_type, codec);
    }
}

fn equivalent_fmtp(want: &str, got: &str) -> bool {
    let mut want_split: Vec<&str> = want.split(";").collect();
    let mut got_split: Vec<&str> = got.split(";").collect();

    if want_split.len() != got_split.len() {
        return false;
    }

    want_split.sort();
    got_split.sort();

    for (i, &want_part) in want_split.iter().enumerate() {
        let want_part = want_part.trim();
        let got_part = got_split[i].trim();
        if got_part != want_part {
            return false;
        }
    }

    return true;
}

pub(crate) fn codecs_match(wanted: &Codec, got: &Codec) -> bool {
    if &wanted.name != "" && wanted.name != got.name {
        return false;
    }
    if wanted.clock_rate != 0 && wanted.clock_rate != got.clock_rate {
        return false;
    }
    if &wanted.encoding_parameters != "" && wanted.encoding_parameters != got.encoding_parameters {
        return false;
    }
    if &wanted.fmtp != "" && !equivalent_fmtp(&wanted.fmtp, &got.fmtp) {
        return false;
    }

    return true;
}

pub struct Lexer<'a, R: io::BufRead> {
    pub desc: SessionDescription,
    pub reader: &'a mut R,
}

pub struct StateFn<'a, R: io::BufRead> {
    pub f: fn(&mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>, Error>,
}

pub fn read_type<R: io::BufRead>(reader: &mut R) -> Result<(String, usize), Error> {
    let mut buf = vec![];
    let num_bytes = reader.read_until(b'=', &mut buf)?;
    if num_bytes == 0 {
        return Ok(("".to_owned(), num_bytes));
    }

    if buf.len() != 2 {
        return Err(Error::new(format!("SyntaxError: {:?}", buf)));
    }
    let key = String::from_utf8(buf)?;
    //print!("{}", key);
    Ok((key, num_bytes))
}

pub fn read_value<R: io::BufRead>(reader: &mut R) -> Result<(String, usize), Error> {
    let mut line = vec![];
    let num_bytes = reader.read_until(b'\n', &mut line)?;
    if num_bytes == 0 {
        return Ok(("".to_owned(), num_bytes));
    }

    let value = String::from_utf8(line)?;
    //print!("{}", value);
    Ok((value.trim().to_string(), num_bytes))
}

pub fn index_of(element: &str, data: &[&str]) -> i32 {
    for (k, &v) in data.iter().enumerate() {
        if element == v {
            return k as i32;
        }
    }
    return -1;
}

pub fn key_value_build(key: &str, value: Option<&String>) -> String {
    if let Some(val) = value {
        format!("{}{}{}", key, val, END_LINE)
    } else {
        "".to_string()
    }
}
