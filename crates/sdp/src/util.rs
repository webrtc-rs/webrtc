#[cfg(test)]
mod util_test;

use super::error::{Error, Result};
use super::session_description::SessionDescription;

use std::collections::HashMap;
use std::io::SeekFrom;
use std::{fmt, io};

pub const END_LINE: &str = "\r\n";
pub const ATTRIBUTE_KEY: &str = "a=";

/// ConnectionRole indicates which of the end points should initiate the connection establishment
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ConnectionRole {
    Unspecified,

    /// ConnectionRoleActive indicates the endpoint will initiate an outgoing connection.
    Active,

    /// ConnectionRolePassive indicates the endpoint will accept an incoming connection.
    Passive,

    /// ConnectionRoleActpass indicates the endpoint is willing to accept an incoming connection or to initiate an outgoing connection.
    Actpass,

    /// ConnectionRoleHoldconn indicates the endpoint does not want the connection to be established for the time being.
    Holdconn,
}

impl Default for ConnectionRole {
    fn default() -> Self {
        ConnectionRole::Unspecified
    }
}

const CONNECTION_ROLE_ACTIVE_STR: &str = "active";
const CONNECTION_ROLE_PASSIVE_STR: &str = "passive";
const CONNECTION_ROLE_ACTPASS_STR: &str = "actpass";
const CONNECTION_ROLE_HOLDCONN_STR: &str = "holdconn";

impl fmt::Display for ConnectionRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ConnectionRole::Active => CONNECTION_ROLE_ACTIVE_STR,
            ConnectionRole::Passive => CONNECTION_ROLE_PASSIVE_STR,
            ConnectionRole::Actpass => CONNECTION_ROLE_ACTPASS_STR,
            ConnectionRole::Holdconn => CONNECTION_ROLE_HOLDCONN_STR,
            _ => "Unspecified",
        };
        write!(f, "{}", s)
    }
}

impl From<u8> for ConnectionRole {
    fn from(v: u8) -> Self {
        match v {
            1 => ConnectionRole::Active,
            2 => ConnectionRole::Passive,
            3 => ConnectionRole::Actpass,
            4 => ConnectionRole::Holdconn,
            _ => ConnectionRole::Unspecified,
        }
    }
}

impl From<&str> for ConnectionRole {
    fn from(raw: &str) -> Self {
        match raw {
            CONNECTION_ROLE_ACTIVE_STR => ConnectionRole::Active,
            CONNECTION_ROLE_PASSIVE_STR => ConnectionRole::Passive,
            CONNECTION_ROLE_ACTPASS_STR => ConnectionRole::Actpass,
            CONNECTION_ROLE_HOLDCONN_STR => ConnectionRole::Holdconn,
            _ => ConnectionRole::Unspecified,
        }
    }
}

/// https://tools.ietf.org/html/draft-ietf-rtcweb-jsep-26#section-5.2.1
/// Session ID is recommended to be constructed by generating a 64-bit
/// quantity with the highest bit set to zero and the remaining 63-bits
/// being cryptographically random.
pub(crate) fn new_session_id() -> u64 {
    let c = u64::MAX ^ (1u64 << 63);
    rand::random::<u64>() & c
}

// Codec represents a codec
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Codec {
    pub payload_type: u8,
    pub name: String,
    pub clock_rate: u32,
    pub encoding_parameters: String,
    pub fmtp: String,
    pub rtcp_feedback: Vec<String>,
}

impl fmt::Display for Codec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}/{}/{} ({}) [{}]",
            self.payload_type,
            self.name,
            self.clock_rate,
            self.encoding_parameters,
            self.fmtp,
            self.rtcp_feedback.join(", "),
        )
    }
}

pub(crate) fn parse_rtpmap(rtpmap: &str) -> Result<Codec> {
    // a=rtpmap:<payload type> <encoding name>/<clock rate>[/<encoding parameters>]
    let split: Vec<&str> = rtpmap.split_whitespace().collect();
    if split.len() != 2 {
        return Err(Error::MissingWhitespace);
    }

    let pt_split: Vec<&str> = split[0].split(':').collect();
    if pt_split.len() != 2 {
        return Err(Error::MissingColon);
    }
    let payload_type = pt_split[1].parse::<u8>()?;

    let split: Vec<&str> = split[1].split('/').collect();
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
        ..Default::default()
    })
}

pub(crate) fn parse_fmtp(fmtp: &str) -> Result<Codec> {
    // a=fmtp:<format> <format specific parameters>
    let split: Vec<&str> = fmtp.split_whitespace().collect();
    if split.len() != 2 {
        return Err(Error::MissingWhitespace);
    }

    let fmtp = split[1].to_string();

    let split: Vec<&str> = split[0].split(':').collect();
    if split.len() != 2 {
        return Err(Error::MissingColon);
    }
    let payload_type = split[1].parse::<u8>()?;

    Ok(Codec {
        payload_type,
        fmtp,
        ..Default::default()
    })
}

pub(crate) fn parse_rtcp_fb(rtcp_fb: &str) -> Result<Codec> {
    // a=ftcp-fb:<payload type> <RTCP feedback type> [<RTCP feedback parameter>]
    let split: Vec<&str> = rtcp_fb.splitn(2, ' ').collect();
    if split.len() != 2 {
        return Err(Error::MissingWhitespace);
    }

    let pt_split: Vec<&str> = split[0].split(':').collect();
    if pt_split.len() != 2 {
        return Err(Error::MissingColon);
    }

    Ok(Codec {
        payload_type: pt_split[1].parse::<u8>()?,
        rtcp_feedback: vec![split[1].to_string()],
        ..Default::default()
    })
}

pub(crate) fn merge_codecs(mut codec: Codec, codecs: &mut HashMap<u8, Codec>) {
    if let Some(saved_codec) = codecs.get_mut(&codec.payload_type) {
        if saved_codec.payload_type == 0 {
            saved_codec.payload_type = codec.payload_type
        }
        if saved_codec.name.is_empty() {
            saved_codec.name = codec.name
        }
        if saved_codec.clock_rate == 0 {
            saved_codec.clock_rate = codec.clock_rate
        }
        if saved_codec.encoding_parameters.is_empty() {
            saved_codec.encoding_parameters = codec.encoding_parameters
        }
        if saved_codec.fmtp.is_empty() {
            saved_codec.fmtp = codec.fmtp
        }
        saved_codec.rtcp_feedback.append(&mut codec.rtcp_feedback);
    } else {
        codecs.insert(codec.payload_type, codec);
    }
}

fn equivalent_fmtp(want: &str, got: &str) -> bool {
    let mut want_split: Vec<&str> = want.split(';').collect();
    let mut got_split: Vec<&str> = got.split(';').collect();

    if want_split.len() != got_split.len() {
        return false;
    }

    want_split.sort_unstable();
    got_split.sort_unstable();

    for (i, &want_part) in want_split.iter().enumerate() {
        let want_part = want_part.trim();
        let got_part = got_split[i].trim();
        if got_part != want_part {
            return false;
        }
    }

    true
}

pub(crate) fn codecs_match(wanted: &Codec, got: &Codec) -> bool {
    if !wanted.name.is_empty() && wanted.name.to_lowercase() != got.name.to_lowercase() {
        return false;
    }
    if wanted.clock_rate != 0 && wanted.clock_rate != got.clock_rate {
        return false;
    }
    if !wanted.encoding_parameters.is_empty()
        && wanted.encoding_parameters != got.encoding_parameters
    {
        return false;
    }
    if !wanted.fmtp.is_empty() && !equivalent_fmtp(&wanted.fmtp, &got.fmtp) {
        return false;
    }

    true
}

pub struct Lexer<'a, R: io::BufRead + io::Seek> {
    pub desc: SessionDescription,
    pub reader: &'a mut R,
}

pub type StateFnType<'a, R> = fn(&mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>>;

pub struct StateFn<'a, R: io::BufRead + io::Seek> {
    pub f: StateFnType<'a, R>,
}

pub fn read_type<R: io::BufRead + io::Seek>(reader: &mut R) -> Result<(String, usize)> {
    loop {
        let mut b = [0; 1];
        if reader.read_exact(&mut b).is_err() {
            return Ok(("".to_owned(), 0));
        }

        if b[0] == b'\n' || b[0] == b'\r' {
            continue;
        }
        reader.seek(SeekFrom::Current(-1))?;

        let mut buf = vec![];
        let num_bytes = reader.read_until(b'=', &mut buf)?;
        if num_bytes == 0 {
            return Ok(("".to_owned(), num_bytes));
        }

        let key = String::from_utf8(buf)?;
        match key.len() {
            2 => return Ok((key, num_bytes)),
            _ => return Err(Error::SdpInvalidSyntax(format!("{:?}", key)).into()),
        }
    }
}

pub fn read_value<R: io::BufRead + io::Seek>(reader: &mut R) -> Result<(String, usize)> {
    let mut value = String::new();
    let num_bytes = reader.read_line(&mut value)?;
    Ok((value.trim().to_string(), num_bytes))
}

pub fn index_of(element: &str, data: &[&str]) -> i32 {
    for (k, &v) in data.iter().enumerate() {
        if element == v {
            return k as i32;
        }
    }
    -1
}

pub fn key_value_build(key: &str, value: Option<&String>) -> String {
    if let Some(val) = value {
        format!("{}{}{}", key, val, END_LINE)
    } else {
        "".to_string()
    }
}
