use std::collections::HashMap;
use std::convert::TryFrom;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fmt, io};

use url::Url;

use super::common::*;
use super::media::*;
use crate::error::{Error, Result};
use crate::lexer::*;
use crate::util::*;

/// Constants for SDP attributes used in JSEP
pub const ATTR_KEY_CANDIDATE: &str = "candidate";
pub const ATTR_KEY_END_OF_CANDIDATES: &str = "end-of-candidates";
pub const ATTR_KEY_IDENTITY: &str = "identity";
pub const ATTR_KEY_GROUP: &str = "group";
pub const ATTR_KEY_SSRC: &str = "ssrc";
pub const ATTR_KEY_SSRCGROUP: &str = "ssrc-group";
pub const ATTR_KEY_MSID: &str = "msid";
pub const ATTR_KEY_MSID_SEMANTIC: &str = "msid-semantic";
pub const ATTR_KEY_CONNECTION_SETUP: &str = "setup";
pub const ATTR_KEY_MID: &str = "mid";
pub const ATTR_KEY_ICELITE: &str = "ice-lite";
pub const ATTR_KEY_RTCPMUX: &str = "rtcp-mux";
pub const ATTR_KEY_RTCPRSIZE: &str = "rtcp-rsize";
pub const ATTR_KEY_INACTIVE: &str = "inactive";
pub const ATTR_KEY_RECV_ONLY: &str = "recvonly";
pub const ATTR_KEY_SEND_ONLY: &str = "sendonly";
pub const ATTR_KEY_SEND_RECV: &str = "sendrecv";
pub const ATTR_KEY_EXT_MAP: &str = "extmap";
pub const ATTR_KEY_EXTMAP_ALLOW_MIXED: &str = "extmap-allow-mixed";

/// Constants for semantic tokens used in JSEP
pub const SEMANTIC_TOKEN_LIP_SYNCHRONIZATION: &str = "LS";
pub const SEMANTIC_TOKEN_FLOW_IDENTIFICATION: &str = "FID";
pub const SEMANTIC_TOKEN_FORWARD_ERROR_CORRECTION: &str = "FEC";
pub const SEMANTIC_TOKEN_WEBRTC_MEDIA_STREAMS: &str = "WMS";

/// Version describes the value provided by the "v=" field which gives
/// the version of the Session Description Protocol.
pub type Version = isize;

/// Origin defines the structure for the "o=" field which provides the
/// originator of the session plus a session identifier and version number.
#[derive(Debug, Default, Clone)]
pub struct Origin {
    pub username: String,
    pub session_id: u64,
    pub session_version: u64,
    pub network_type: String,
    pub address_type: String,
    pub unicast_address: String,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {} {} {}",
            self.username,
            self.session_id,
            self.session_version,
            self.network_type,
            self.address_type,
            self.unicast_address,
        )
    }
}

impl Origin {
    pub fn new() -> Self {
        Origin {
            username: "".to_owned(),
            session_id: 0,
            session_version: 0,
            network_type: "".to_owned(),
            address_type: "".to_owned(),
            unicast_address: "".to_owned(),
        }
    }
}

/// SessionName describes a structured representations for the "s=" field
/// and is the textual session name.
pub type SessionName = String;

/// EmailAddress describes a structured representations for the "e=" line
/// which specifies email contact information for the person responsible for
/// the conference.
pub type EmailAddress = String;

/// PhoneNumber describes a structured representations for the "p=" line
/// specify phone contact information for the person responsible for the
/// conference.
pub type PhoneNumber = String;

/// TimeZone defines the structured object for "z=" line which describes
/// repeated sessions scheduling.
#[derive(Debug, Default, Clone)]
pub struct TimeZone {
    pub adjustment_time: u64,
    pub offset: i64,
}

impl fmt::Display for TimeZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.adjustment_time, self.offset)
    }
}

/// TimeDescription describes "t=", "r=" fields of the session description
/// which are used to specify the start and stop times for a session as well as
/// repeat intervals and durations for the scheduled session.
#[derive(Debug, Default, Clone)]
pub struct TimeDescription {
    /// `t=<start-time> <stop-time>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.9>
    pub timing: Timing,

    /// `r=<repeat interval> <active duration> <offsets from start-time>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.10>
    pub repeat_times: Vec<RepeatTime>,
}

/// Timing defines the "t=" field's structured representation for the start and
/// stop times.
#[derive(Debug, Default, Clone)]
pub struct Timing {
    pub start_time: u64,
    pub stop_time: u64,
}

impl fmt::Display for Timing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.start_time, self.stop_time)
    }
}

/// RepeatTime describes the "r=" fields of the session description which
/// represents the intervals and durations for repeated scheduled sessions.
#[derive(Debug, Default, Clone)]
pub struct RepeatTime {
    pub interval: i64,
    pub duration: i64,
    pub offsets: Vec<i64>,
}

impl fmt::Display for RepeatTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.interval, self.duration)?;

        for value in &self.offsets {
            write!(f, " {value}")?;
        }
        Ok(())
    }
}

/// SessionDescription is a a well-defined format for conveying sufficient
/// information to discover and participate in a multimedia session.
#[derive(Debug, Default, Clone)]
pub struct SessionDescription {
    /// `v=0`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.1>
    pub version: Version,

    /// `o=<username> <sess-id> <sess-version> <nettype> <addrtype> <unicast-address>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.2>
    pub origin: Origin,

    /// `s=<session name>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.3>
    pub session_name: SessionName,

    /// `i=<session description>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.4>
    pub session_information: Option<Information>,

    /// `u=<uri>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.5>
    pub uri: Option<Url>,

    /// `e=<email-address>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.6>
    pub email_address: Option<EmailAddress>,

    /// `p=<phone-number>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.6>
    pub phone_number: Option<PhoneNumber>,

    /// `c=<nettype> <addrtype> <connection-address>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.7>
    pub connection_information: Option<ConnectionInformation>,

    /// `b=<bwtype>:<bandwidth>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.8>
    pub bandwidth: Vec<Bandwidth>,

    /// <https://tools.ietf.org/html/rfc4566#section-5.9>
    /// <https://tools.ietf.org/html/rfc4566#section-5.10>
    pub time_descriptions: Vec<TimeDescription>,

    /// `z=<adjustment time> <offset> <adjustment time> <offset> ...`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.11>
    pub time_zones: Vec<TimeZone>,

    /// `k=<method>`
    ///
    /// `k=<method>:<encryption key>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.12>
    pub encryption_key: Option<EncryptionKey>,

    /// `a=<attribute>`
    ///
    /// `a=<attribute>:<value>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.13>
    pub attributes: Vec<Attribute>,

    /// <https://tools.ietf.org/html/rfc4566#section-5.14>
    pub media_descriptions: Vec<MediaDescription>,
}

impl fmt::Display for SessionDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_key_value(f, "v=", Some(&self.version))?;
        write_key_value(f, "o=", Some(&self.origin))?;
        write_key_value(f, "s=", Some(&self.session_name))?;

        write_key_value(f, "i=", self.session_information.as_ref())?;

        if let Some(uri) = &self.uri {
            write_key_value(f, "u=", Some(uri))?;
        }
        write_key_value(f, "e=", self.email_address.as_ref())?;
        write_key_value(f, "p=", self.phone_number.as_ref())?;
        if let Some(connection_information) = &self.connection_information {
            write_key_value(f, "c=", Some(&connection_information))?;
        }

        for bandwidth in &self.bandwidth {
            write_key_value(f, "b=", Some(&bandwidth))?;
        }
        for time_description in &self.time_descriptions {
            write_key_value(f, "t=", Some(&time_description.timing))?;
            for repeat_time in &time_description.repeat_times {
                write_key_value(f, "r=", Some(&repeat_time))?;
            }
        }

        write_key_slice_of_values(f, "z=", &self.time_zones)?;

        write_key_value(f, "k=", self.encryption_key.as_ref())?;
        for attribute in &self.attributes {
            write_key_value(f, "a=", Some(&attribute))?;
        }

        for media_description in &self.media_descriptions {
            write_key_value(f, "m=", Some(&media_description.media_name))?;
            write_key_value(f, "i=", media_description.media_title.as_ref())?;
            if let Some(connection_information) = &media_description.connection_information {
                write_key_value(f, "c=", Some(&connection_information))?;
            }
            for bandwidth in &media_description.bandwidth {
                write_key_value(f, "b=", Some(&bandwidth))?;
            }
            write_key_value(f, "k=", media_description.encryption_key.as_ref())?;
            for attribute in &media_description.attributes {
                write_key_value(f, "a=", Some(&attribute))?;
            }
        }

        Ok(())
    }
}

/// Reset cleans the SessionDescription, and sets all fields back to their default values
impl SessionDescription {
    /// API to match draft-ietf-rtcweb-jsep
    /// Move to webrtc or its own package?

    /// NewJSEPSessionDescription creates a new SessionDescription with
    /// some settings that are required by the JSEP spec.
    pub fn new_jsep_session_description(identity: bool) -> Self {
        let d = SessionDescription {
            version: 0,
            origin: Origin {
                username: "-".to_string(),
                session_id: new_session_id(),
                session_version: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_else(|_| Duration::from_secs(0))
                    .subsec_nanos() as u64,
                network_type: "IN".to_string(),
                address_type: "IP4".to_string(),
                unicast_address: "0.0.0.0".to_string(),
            },
            session_name: "-".to_string(),
            session_information: None,
            uri: None,
            email_address: None,
            phone_number: None,
            connection_information: None,
            bandwidth: vec![],
            time_descriptions: vec![TimeDescription {
                timing: Timing {
                    start_time: 0,
                    stop_time: 0,
                },
                repeat_times: vec![],
            }],
            time_zones: vec![],
            encryption_key: None,
            attributes: vec![], // TODO: implement trickle ICE
            media_descriptions: vec![],
        };

        if identity {
            d.with_property_attribute(ATTR_KEY_IDENTITY.to_string())
        } else {
            d
        }
    }

    /// WithPropertyAttribute adds a property attribute 'a=key' to the session description
    pub fn with_property_attribute(mut self, key: String) -> Self {
        self.attributes.push(Attribute::new(key, None));
        self
    }

    /// WithValueAttribute adds a value attribute 'a=key:value' to the session description
    pub fn with_value_attribute(mut self, key: String, value: String) -> Self {
        self.attributes.push(Attribute::new(key, Some(value)));
        self
    }

    /// WithFingerprint adds a fingerprint to the session description
    pub fn with_fingerprint(self, algorithm: String, value: String) -> Self {
        self.with_value_attribute("fingerprint".to_string(), algorithm + " " + value.as_str())
    }

    /// WithMedia adds a media description to the session description
    pub fn with_media(mut self, md: MediaDescription) -> Self {
        self.media_descriptions.push(md);
        self
    }

    fn build_codec_map(&self) -> HashMap<u8, Codec> {
        let mut codecs: HashMap<u8, Codec> = HashMap::new();

        for m in &self.media_descriptions {
            for a in &m.attributes {
                let attr = a.to_string();
                if attr.starts_with("rtpmap:") {
                    if let Ok(codec) = parse_rtpmap(&attr) {
                        merge_codecs(codec, &mut codecs);
                    }
                } else if attr.starts_with("fmtp:") {
                    if let Ok(codec) = parse_fmtp(&attr) {
                        merge_codecs(codec, &mut codecs);
                    }
                } else if attr.starts_with("rtcp-fb:") {
                    if let Ok(codec) = parse_rtcp_fb(&attr) {
                        merge_codecs(codec, &mut codecs);
                    }
                }
            }
        }

        codecs
    }

    /// get_codec_for_payload_type scans the SessionDescription for the given payload type and returns the codec
    pub fn get_codec_for_payload_type(&self, payload_type: u8) -> Result<Codec> {
        let codecs = self.build_codec_map();

        if let Some(codec) = codecs.get(&payload_type) {
            Ok(codec.clone())
        } else {
            Err(Error::PayloadTypeNotFound)
        }
    }

    /// get_payload_type_for_codec scans the SessionDescription for a codec that matches the provided codec
    /// as closely as possible and returns its payload type
    pub fn get_payload_type_for_codec(&self, wanted: &Codec) -> Result<u8> {
        let codecs = self.build_codec_map();

        for (payload_type, codec) in codecs.iter() {
            if codecs_match(wanted, codec) {
                return Ok(*payload_type);
            }
        }

        Err(Error::CodecNotFound)
    }

    /// Returns whether an attribute exists
    pub fn has_attribute(&self, key: &str) -> bool {
        self.attributes.iter().any(|a| a.key == key)
    }

    /// Attribute returns the value of an attribute and if it exists
    pub fn attribute(&self, key: &str) -> Option<&String> {
        for a in &self.attributes {
            if a.key == key {
                return a.value.as_ref();
            }
        }
        None
    }

    /// Marshal takes a SDP struct to text
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5>
    ///
    /// Session description
    ///    v=  (protocol version)
    ///    o=  (originator and session identifier)
    ///    s=  (session name)
    ///    i=* (session information)
    ///    u=* (URI of description)
    ///    e=* (email address)
    ///    p=* (phone number)
    ///    c=* (connection information -- not required if included in
    ///         all media)
    ///    b=* (zero or more bandwidth information lines)
    ///    One or more time descriptions ("t=" and "r=" lines; see below)
    ///    z=* (time zone adjustments)
    ///    k=* (encryption key)
    ///    a=* (zero or more session attribute lines)
    ///    Zero or more media descriptions
    ///
    /// Time description
    ///    t=  (time the session is active)
    ///    r=* (zero or more repeat times)
    ///
    /// Media description, if present
    ///    m=  (media name and transport address)
    ///    i=* (media title)
    ///    c=* (connection information -- optional if included at
    ///         session level)
    ///    b=* (zero or more bandwidth information lines)
    ///    k=* (encryption key)
    ///    a=* (zero or more media attribute lines)
    pub fn marshal(&self) -> String {
        self.to_string()
    }

    /// Unmarshal is the primary function that deserializes the session description
    /// message and stores it inside of a structured SessionDescription object.
    ///
    /// The States Transition Table describes the computation flow between functions
    /// (namely s1, s2, s3, ...) for a parsing procedure that complies with the
    /// specifications laid out by the rfc4566#section-5 as well as by JavaScript
    /// Session Establishment Protocol draft. Links:
    ///     <https://tools.ietf.org/html/rfc4566#section-5>
    ///     <https://tools.ietf.org/html/draft-ietf-rtcweb-jsep-24>
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5>
    ///
    /// Session description
    ///    v=  (protocol version)
    ///    o=  (originator and session identifier)
    ///    s=  (session name)
    ///    i=* (session information)
    ///    u=* (URI of description)
    ///    e=* (email address)
    ///    p=* (phone number)
    ///    c=* (connection information -- not required if included in
    ///         all media)
    ///    b=* (zero or more bandwidth information lines)
    ///    One or more time descriptions ("t=" and "r=" lines; see below)
    ///    z=* (time zone adjustments)
    ///    k=* (encryption key)
    ///    a=* (zero or more session attribute lines)
    ///    Zero or more media descriptions
    ///
    /// Time description
    ///    t=  (time the session is active)
    ///    r=* (zero or more repeat times)
    ///
    /// Media description, if present
    ///    m=  (media name and transport address)
    ///    i=* (media title)
    ///    c=* (connection information -- optional if included at
    ///         session level)
    ///    b=* (zero or more bandwidth information lines)
    ///    k=* (encryption key)
    ///    a=* (zero or more media attribute lines)
    ///
    /// In order to generate the following state table and draw subsequent
    /// deterministic finite-state automota ("DFA") the following regex was used to
    /// derive the DFA:
    ///    vosi?u?e?p?c?b*(tr*)+z?k?a*(mi?c?b*k?a*)*
    /// possible place and state to exit:
    ///                    **   * * *  ** * * * *
    ///                    99   1 1 1  11 1 1 1 1
    ///                         3 1 1  26 5 5 4 4
    ///
    /// Please pay close attention to the `k`, and `a` parsing states. In the table
    /// below in order to distinguish between the states belonging to the media
    /// description as opposed to the session description, the states are marked
    /// with an asterisk ("a*", "k*").
    ///
    /// ```ignore
    /// +--------+----+-------+----+-----+----+-----+---+----+----+---+---+-----+---+---+----+---+----+
    /// | STATES | a* | a*,k* | a  | a,k | b  | b,c | e | i  | m  | o | p | r,t | s | t | u  | v | z  |
    /// +--------+----+-------+----+-----+----+-----+---+----+----+---+---+-----+---+---+----+---+----+
    /// |   s1   |    |       |    |     |    |     |   |    |    |   |   |     |   |   |    | 2 |    |
    /// |   s2   |    |       |    |     |    |     |   |    |    | 3 |   |     |   |   |    |   |    |
    /// |   s3   |    |       |    |     |    |     |   |    |    |   |   |     | 4 |   |    |   |    |
    /// |   s4   |    |       |    |     |    |   5 | 6 |  7 |    |   | 8 |     |   | 9 | 10 |   |    |
    /// |   s5   |    |       |    |     |  5 |     |   |    |    |   |   |     |   | 9 |    |   |    |
    /// |   s6   |    |       |    |     |    |   5 |   |    |    |   | 8 |     |   | 9 |    |   |    |
    /// |   s7   |    |       |    |     |    |   5 | 6 |    |    |   | 8 |     |   | 9 | 10 |   |    |
    /// |   s8   |    |       |    |     |    |   5 |   |    |    |   |   |     |   | 9 |    |   |    |
    /// |   s9   |    |       |    |  11 |    |     |   |    | 12 |   |   |   9 |   |   |    |   | 13 |
    /// |   s10  |    |       |    |     |    |   5 | 6 |    |    |   | 8 |     |   | 9 |    |   |    |
    /// |   s11  |    |       | 11 |     |    |     |   |    | 12 |   |   |     |   |   |    |   |    |
    /// |   s12  |    |    14 |    |     |    |  15 |   | 16 | 12 |   |   |     |   |   |    |   |    |
    /// |   s13  |    |       |    |  11 |    |     |   |    | 12 |   |   |     |   |   |    |   |    |
    /// |   s14  | 14 |       |    |     |    |     |   |    | 12 |   |   |     |   |   |    |   |    |
    /// |   s15  |    |    14 |    |     | 15 |     |   |    | 12 |   |   |     |   |   |    |   |    |
    /// |   s16  |    |    14 |    |     |    |  15 |   |    | 12 |   |   |     |   |   |    |   |    |
    /// +--------+----+-------+----+-----+----+-----+---+----+----+---+---+-----+---+---+----+---+----+
    /// ```
    pub fn unmarshal<R: io::BufRead + io::Seek>(reader: &mut R) -> Result<Self> {
        let mut lexer = Lexer {
            desc: SessionDescription {
                version: 0,
                origin: Origin::new(),
                session_name: "".to_owned(),
                session_information: None,
                uri: None,
                email_address: None,
                phone_number: None,
                connection_information: None,
                bandwidth: vec![],
                time_descriptions: vec![],
                time_zones: vec![],
                encryption_key: None,
                attributes: vec![],
                media_descriptions: vec![],
            },
            reader,
        };

        let mut state = Some(StateFn { f: s1 });
        while let Some(s) = state {
            state = (s.f)(&mut lexer)?;
        }

        Ok(lexer.desc)
    }
}

impl From<SessionDescription> for String {
    fn from(sdp: SessionDescription) -> String {
        sdp.marshal()
    }
}

impl TryFrom<String> for SessionDescription {
    type Error = Error;
    fn try_from(sdp_string: String) -> Result<Self> {
        let mut reader = io::Cursor::new(sdp_string.as_bytes());
        let session_description = SessionDescription::unmarshal(&mut reader)?;
        Ok(session_description)
    }
}

fn s1<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, _) = read_type(lexer.reader)?;
    if &key == b"v=" {
        return Ok(Some(StateFn {
            f: unmarshal_protocol_version,
        }));
    }

    Err(Error::SdpInvalidSyntax(String::from_utf8(key)?))
}

fn s2<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, _) = read_type(lexer.reader)?;
    if &key == b"o=" {
        return Ok(Some(StateFn {
            f: unmarshal_origin,
        }));
    }

    Err(Error::SdpInvalidSyntax(String::from_utf8(key)?))
}

fn s3<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, _) = read_type(lexer.reader)?;
    if &key == b"s=" {
        return Ok(Some(StateFn {
            f: unmarshal_session_name,
        }));
    }

    Err(Error::SdpInvalidSyntax(String::from_utf8(key)?))
}

fn s4<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, _) = read_type(lexer.reader)?;
    match key.as_slice() {
        b"i=" => Ok(Some(StateFn {
            f: unmarshal_session_information,
        })),
        b"u=" => Ok(Some(StateFn { f: unmarshal_uri })),
        b"e=" => Ok(Some(StateFn { f: unmarshal_email })),
        b"p=" => Ok(Some(StateFn { f: unmarshal_phone })),
        b"c=" => Ok(Some(StateFn {
            f: unmarshal_session_connection_information,
        })),
        b"b=" => Ok(Some(StateFn {
            f: unmarshal_session_bandwidth,
        })),
        b"t=" => Ok(Some(StateFn {
            f: unmarshal_timing,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s5<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, _) = read_type(lexer.reader)?;
    match key.as_slice() {
        b"b=" => Ok(Some(StateFn {
            f: unmarshal_session_bandwidth,
        })),
        b"t=" => Ok(Some(StateFn {
            f: unmarshal_timing,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s6<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, _) = read_type(lexer.reader)?;
    match key.as_slice() {
        b"p=" => Ok(Some(StateFn { f: unmarshal_phone })),
        b"c=" => Ok(Some(StateFn {
            f: unmarshal_session_connection_information,
        })),
        b"b=" => Ok(Some(StateFn {
            f: unmarshal_session_bandwidth,
        })),
        b"t=" => Ok(Some(StateFn {
            f: unmarshal_timing,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s7<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, _) = read_type(lexer.reader)?;
    match key.as_slice() {
        b"u=" => Ok(Some(StateFn { f: unmarshal_uri })),
        b"e=" => Ok(Some(StateFn { f: unmarshal_email })),
        b"p=" => Ok(Some(StateFn { f: unmarshal_phone })),
        b"c=" => Ok(Some(StateFn {
            f: unmarshal_session_connection_information,
        })),
        b"b=" => Ok(Some(StateFn {
            f: unmarshal_session_bandwidth,
        })),
        b"t=" => Ok(Some(StateFn {
            f: unmarshal_timing,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s8<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, _) = read_type(lexer.reader)?;
    match key.as_slice() {
        b"c=" => Ok(Some(StateFn {
            f: unmarshal_session_connection_information,
        })),
        b"b=" => Ok(Some(StateFn {
            f: unmarshal_session_bandwidth,
        })),
        b"t=" => Ok(Some(StateFn {
            f: unmarshal_timing,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s9<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, num_bytes) = read_type(lexer.reader)?;
    if key.is_empty() && num_bytes == 0 {
        return Ok(None);
    }

    match key.as_slice() {
        b"z=" => Ok(Some(StateFn {
            f: unmarshal_time_zones,
        })),
        b"k=" => Ok(Some(StateFn {
            f: unmarshal_session_encryption_key,
        })),
        b"a=" => Ok(Some(StateFn {
            f: unmarshal_session_attribute,
        })),
        b"r=" => Ok(Some(StateFn {
            f: unmarshal_repeat_times,
        })),
        b"t=" => Ok(Some(StateFn {
            f: unmarshal_timing,
        })),
        b"m=" => Ok(Some(StateFn {
            f: unmarshal_media_description,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s10<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, _) = read_type(lexer.reader)?;
    match key.as_slice() {
        b"e=" => Ok(Some(StateFn { f: unmarshal_email })),
        b"p=" => Ok(Some(StateFn { f: unmarshal_phone })),
        b"c=" => Ok(Some(StateFn {
            f: unmarshal_session_connection_information,
        })),
        b"b=" => Ok(Some(StateFn {
            f: unmarshal_session_bandwidth,
        })),
        b"t=" => Ok(Some(StateFn {
            f: unmarshal_timing,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s11<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, num_bytes) = read_type(lexer.reader)?;
    if key.is_empty() && num_bytes == 0 {
        return Ok(None);
    }

    match key.as_slice() {
        b"a=" => Ok(Some(StateFn {
            f: unmarshal_session_attribute,
        })),
        b"m=" => Ok(Some(StateFn {
            f: unmarshal_media_description,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s12<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, num_bytes) = read_type(lexer.reader)?;
    if key.is_empty() && num_bytes == 0 {
        return Ok(None);
    }

    match key.as_slice() {
        b"a=" => Ok(Some(StateFn {
            f: unmarshal_media_attribute,
        })),
        b"k=" => Ok(Some(StateFn {
            f: unmarshal_media_encryption_key,
        })),
        b"b=" => Ok(Some(StateFn {
            f: unmarshal_media_bandwidth,
        })),
        b"c=" => Ok(Some(StateFn {
            f: unmarshal_media_connection_information,
        })),
        b"i=" => Ok(Some(StateFn {
            f: unmarshal_media_title,
        })),
        b"m=" => Ok(Some(StateFn {
            f: unmarshal_media_description,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s13<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, num_bytes) = read_type(lexer.reader)?;
    if key.is_empty() && num_bytes == 0 {
        return Ok(None);
    }

    match key.as_slice() {
        b"a=" => Ok(Some(StateFn {
            f: unmarshal_session_attribute,
        })),
        b"k=" => Ok(Some(StateFn {
            f: unmarshal_session_encryption_key,
        })),
        b"m=" => Ok(Some(StateFn {
            f: unmarshal_media_description,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s14<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, num_bytes) = read_type(lexer.reader)?;
    if key.is_empty() && num_bytes == 0 {
        return Ok(None);
    }

    match key.as_slice() {
        b"a=" => Ok(Some(StateFn {
            f: unmarshal_media_attribute,
        })),
        // Non-spec ordering
        b"k=" => Ok(Some(StateFn {
            f: unmarshal_media_encryption_key,
        })),
        // Non-spec ordering
        b"b=" => Ok(Some(StateFn {
            f: unmarshal_media_bandwidth,
        })),
        // Non-spec ordering
        b"c=" => Ok(Some(StateFn {
            f: unmarshal_media_connection_information,
        })),
        // Non-spec ordering
        b"i=" => Ok(Some(StateFn {
            f: unmarshal_media_title,
        })),
        b"m=" => Ok(Some(StateFn {
            f: unmarshal_media_description,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s15<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, num_bytes) = read_type(lexer.reader)?;
    if key.is_empty() && num_bytes == 0 {
        return Ok(None);
    }

    match key.as_slice() {
        b"a=" => Ok(Some(StateFn {
            f: unmarshal_media_attribute,
        })),
        b"k=" => Ok(Some(StateFn {
            f: unmarshal_media_encryption_key,
        })),
        b"b=" => Ok(Some(StateFn {
            f: unmarshal_media_bandwidth,
        })),
        b"c=" => Ok(Some(StateFn {
            f: unmarshal_media_connection_information,
        })),
        // Non-spec ordering
        b"i=" => Ok(Some(StateFn {
            f: unmarshal_media_title,
        })),
        b"m=" => Ok(Some(StateFn {
            f: unmarshal_media_description,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn s16<'a, R: io::BufRead + io::Seek>(lexer: &mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>> {
    let (key, num_bytes) = read_type(lexer.reader)?;
    if key.is_empty() && num_bytes == 0 {
        return Ok(None);
    }

    match key.as_slice() {
        b"a=" => Ok(Some(StateFn {
            f: unmarshal_media_attribute,
        })),
        b"k=" => Ok(Some(StateFn {
            f: unmarshal_media_encryption_key,
        })),
        b"c=" => Ok(Some(StateFn {
            f: unmarshal_media_connection_information,
        })),
        b"b=" => Ok(Some(StateFn {
            f: unmarshal_media_bandwidth,
        })),
        // Non-spec ordering
        b"i=" => Ok(Some(StateFn {
            f: unmarshal_media_title,
        })),
        b"m=" => Ok(Some(StateFn {
            f: unmarshal_media_description,
        })),
        _ => Err(Error::SdpInvalidSyntax(String::from_utf8(key)?)),
    }
}

fn unmarshal_protocol_version<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    let version = value.parse::<u32>()?;

    // As off the latest draft of the rfc this value is required to be 0.
    // https://tools.ietf.org/html/draft-ietf-rtcweb-jsep-24#section-5.8.1
    if version != 0 {
        return Err(Error::SdpInvalidSyntax(value));
    }

    Ok(Some(StateFn { f: s2 }))
}

fn unmarshal_origin<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    let fields: Vec<&str> = value.split_whitespace().collect();
    if fields.len() != 6 {
        return Err(Error::SdpInvalidSyntax(format!("`o={value}`")));
    }

    let session_id = fields[1].parse::<u64>()?;
    let session_version = fields[2].parse::<u64>()?;

    // Set according to currently registered with IANA
    // https://tools.ietf.org/html/rfc4566#section-8.2.6
    let i = index_of(fields[3], &["IN"]);
    if i == -1 {
        return Err(Error::SdpInvalidValue(fields[3].to_owned()));
    }

    // Set according to currently registered with IANA
    // https://tools.ietf.org/html/rfc4566#section-8.2.7
    let i = index_of(fields[4], &["IP4", "IP6"]);
    if i == -1 {
        return Err(Error::SdpInvalidValue(fields[4].to_owned()));
    }

    // TODO validated UnicastAddress

    lexer.desc.origin = Origin {
        username: fields[0].to_owned(),
        session_id,
        session_version,
        network_type: fields[3].to_owned(),
        address_type: fields[4].to_owned(),
        unicast_address: fields[5].to_owned(),
    };

    Ok(Some(StateFn { f: s3 }))
}

fn unmarshal_session_name<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;
    lexer.desc.session_name = value;
    Ok(Some(StateFn { f: s4 }))
}

fn unmarshal_session_information<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;
    lexer.desc.session_information = Some(value);
    Ok(Some(StateFn { f: s7 }))
}

fn unmarshal_uri<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;
    lexer.desc.uri = Some(Url::parse(&value)?);
    Ok(Some(StateFn { f: s10 }))
}

fn unmarshal_email<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;
    lexer.desc.email_address = Some(value);
    Ok(Some(StateFn { f: s6 }))
}

fn unmarshal_phone<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;
    lexer.desc.phone_number = Some(value);
    Ok(Some(StateFn { f: s8 }))
}

fn unmarshal_session_connection_information<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;
    lexer.desc.connection_information = unmarshal_connection_information(&value)?;
    Ok(Some(StateFn { f: s5 }))
}

fn unmarshal_connection_information(value: &str) -> Result<Option<ConnectionInformation>> {
    let fields: Vec<&str> = value.split_whitespace().collect();
    if fields.len() < 2 {
        return Err(Error::SdpInvalidSyntax(format!("`c={value}`")));
    }

    // Set according to currently registered with IANA
    // https://tools.ietf.org/html/rfc4566#section-8.2.6
    let i = index_of(fields[0], &["IN"]);
    if i == -1 {
        return Err(Error::SdpInvalidValue(fields[0].to_owned()));
    }

    // Set according to currently registered with IANA
    // https://tools.ietf.org/html/rfc4566#section-8.2.7
    let i = index_of(fields[1], &["IP4", "IP6"]);
    if i == -1 {
        return Err(Error::SdpInvalidValue(fields[1].to_owned()));
    }

    let address = if fields.len() > 2 {
        Some(Address {
            address: fields[2].to_owned(),
            ttl: None,
            range: None,
        })
    } else {
        None
    };

    Ok(Some(ConnectionInformation {
        network_type: fields[0].to_owned(),
        address_type: fields[1].to_owned(),
        address,
    }))
}

fn unmarshal_session_bandwidth<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;
    lexer.desc.bandwidth.push(unmarshal_bandwidth(&value)?);
    Ok(Some(StateFn { f: s5 }))
}

fn unmarshal_bandwidth(value: &str) -> Result<Bandwidth> {
    let mut parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 2 {
        return Err(Error::SdpInvalidSyntax(format!("`b={value}`")));
    }

    let experimental = parts[0].starts_with("X-");
    if experimental {
        parts[0] = parts[0].trim_start_matches("X-");
    } else {
        // Set according to currently registered with IANA
        // https://tools.ietf.org/html/rfc4566#section-5.8 and
        // https://datatracker.ietf.org/doc/html/rfc3890
        let i = index_of(parts[0], &["CT", "AS", "TIAS"]);
        if i == -1 {
            return Err(Error::SdpInvalidValue(parts[0].to_owned()));
        }
    }

    let bandwidth = parts[1].parse::<u64>()?;

    Ok(Bandwidth {
        experimental,
        bandwidth_type: parts[0].to_owned(),
        bandwidth,
    })
}

fn unmarshal_timing<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    let fields: Vec<&str> = value.split_whitespace().collect();
    if fields.len() < 2 {
        return Err(Error::SdpInvalidSyntax(format!("`t={value}`")));
    }

    let start_time = fields[0].parse::<u64>()?;
    let stop_time = fields[1].parse::<u64>()?;

    lexer.desc.time_descriptions.push(TimeDescription {
        timing: Timing {
            start_time,
            stop_time,
        },
        repeat_times: vec![],
    });

    Ok(Some(StateFn { f: s9 }))
}

fn unmarshal_repeat_times<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    let fields: Vec<&str> = value.split_whitespace().collect();
    if fields.len() < 3 {
        return Err(Error::SdpInvalidSyntax(format!("`r={value}`")));
    }

    if let Some(latest_time_desc) = lexer.desc.time_descriptions.last_mut() {
        let interval = parse_time_units(fields[0])?;
        let duration = parse_time_units(fields[1])?;
        let mut offsets = vec![];
        for field in fields.iter().skip(2) {
            let offset = parse_time_units(field)?;
            offsets.push(offset);
        }
        latest_time_desc.repeat_times.push(RepeatTime {
            interval,
            duration,
            offsets,
        });

        Ok(Some(StateFn { f: s9 }))
    } else {
        Err(Error::SdpEmptyTimeDescription)
    }
}

fn unmarshal_time_zones<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    // These fields are transimitted in pairs
    // z=<adjustment time> <offset> <adjustment time> <offset> ....
    // so we are making sure that there are actually multiple of 2 total.
    let fields: Vec<&str> = value.split_whitespace().collect();
    if fields.len() % 2 != 0 {
        return Err(Error::SdpInvalidSyntax(format!("`t={value}`")));
    }

    for i in (0..fields.len()).step_by(2) {
        let adjustment_time = fields[i].parse::<u64>()?;
        let offset = parse_time_units(fields[i + 1])?;

        lexer.desc.time_zones.push(TimeZone {
            adjustment_time,
            offset,
        });
    }

    Ok(Some(StateFn { f: s13 }))
}

fn unmarshal_session_encryption_key<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;
    lexer.desc.encryption_key = Some(value);
    Ok(Some(StateFn { f: s11 }))
}

fn unmarshal_session_attribute<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    let fields: Vec<&str> = value.splitn(2, ':').collect();
    let attribute = if fields.len() == 2 {
        Attribute {
            key: fields[0].to_owned(),
            value: Some(fields[1].to_owned()),
        }
    } else {
        Attribute {
            key: fields[0].to_owned(),
            value: None,
        }
    };
    lexer.desc.attributes.push(attribute);

    Ok(Some(StateFn { f: s11 }))
}

fn unmarshal_media_description<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    let fields: Vec<&str> = value.split_whitespace().collect();
    if fields.len() < 4 {
        return Err(Error::SdpInvalidSyntax(format!("`m={value}`")));
    }

    // <media>
    // Set according to currently registered with IANA
    // https://tools.ietf.org/html/rfc4566#section-5.14
    // including "image", registered here:
    // https://datatracker.ietf.org/doc/html/rfc6466
    let i = index_of(
        fields[0],
        &["audio", "video", "text", "application", "message", "image"],
    );
    if i == -1 {
        return Err(Error::SdpInvalidValue(fields[0].to_owned()));
    }

    // <port>
    let parts: Vec<&str> = fields[1].split('/').collect();
    let port_value = parts[0].parse::<u16>()? as isize;
    let port_range = if parts.len() > 1 {
        Some(parts[1].parse::<i32>()? as isize)
    } else {
        None
    };

    // <proto>
    // Set according to currently registered with IANA
    // https://tools.ietf.org/html/rfc4566#section-5.14
    let mut protos = vec![];
    for proto in fields[2].split('/').collect::<Vec<&str>>() {
        let i = index_of(
            proto,
            &[
                "UDP", "RTP", "AVP", "SAVP", "SAVPF", "TLS", "DTLS", "SCTP", "AVPF", "udptl",
            ],
        );
        if i == -1 {
            return Err(Error::SdpInvalidValue(fields[2].to_owned()));
        }
        protos.push(proto.to_owned());
    }

    // <fmt>...
    let mut formats = vec![];
    for field in fields.iter().skip(3) {
        formats.push(field.to_string());
    }

    lexer.desc.media_descriptions.push(MediaDescription {
        media_name: MediaName {
            media: fields[0].to_owned(),
            port: RangedPort {
                value: port_value,
                range: port_range,
            },
            protos,
            formats,
        },
        media_title: None,
        connection_information: None,
        bandwidth: vec![],
        encryption_key: None,
        attributes: vec![],
    });

    Ok(Some(StateFn { f: s12 }))
}

fn unmarshal_media_title<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    if let Some(latest_media_desc) = lexer.desc.media_descriptions.last_mut() {
        latest_media_desc.media_title = Some(value);
        Ok(Some(StateFn { f: s16 }))
    } else {
        Err(Error::SdpEmptyTimeDescription)
    }
}

fn unmarshal_media_connection_information<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    if let Some(latest_media_desc) = lexer.desc.media_descriptions.last_mut() {
        latest_media_desc.connection_information = unmarshal_connection_information(&value)?;
        Ok(Some(StateFn { f: s15 }))
    } else {
        Err(Error::SdpEmptyTimeDescription)
    }
}

fn unmarshal_media_bandwidth<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    if let Some(latest_media_desc) = lexer.desc.media_descriptions.last_mut() {
        let bandwidth = unmarshal_bandwidth(&value)?;
        latest_media_desc.bandwidth.push(bandwidth);
        Ok(Some(StateFn { f: s15 }))
    } else {
        Err(Error::SdpEmptyTimeDescription)
    }
}

fn unmarshal_media_encryption_key<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    if let Some(latest_media_desc) = lexer.desc.media_descriptions.last_mut() {
        latest_media_desc.encryption_key = Some(value);
        Ok(Some(StateFn { f: s14 }))
    } else {
        Err(Error::SdpEmptyTimeDescription)
    }
}

fn unmarshal_media_attribute<'a, R: io::BufRead + io::Seek>(
    lexer: &mut Lexer<'a, R>,
) -> Result<Option<StateFn<'a, R>>> {
    let (value, _) = read_value(lexer.reader)?;

    let fields: Vec<&str> = value.splitn(2, ':').collect();
    let attribute = if fields.len() == 2 {
        Attribute {
            key: fields[0].to_owned(),
            value: Some(fields[1].to_owned()),
        }
    } else {
        Attribute {
            key: fields[0].to_owned(),
            value: None,
        }
    };

    if let Some(latest_media_desc) = lexer.desc.media_descriptions.last_mut() {
        latest_media_desc.attributes.push(attribute);
        Ok(Some(StateFn { f: s14 }))
    } else {
        Err(Error::SdpEmptyTimeDescription)
    }
}

fn parse_time_units(value: &str) -> Result<i64> {
    // Some time offsets in the protocol can be provided with a shorthand
    // notation. This code ensures to convert it to NTP timestamp format.
    let val = value.as_bytes();
    let len = val.len();
    let (num, factor) = match val.last() {
        Some(b'd') => (&value[..len - 1], 86400), // days
        Some(b'h') => (&value[..len - 1], 3600),  // hours
        Some(b'm') => (&value[..len - 1], 60),    // minutes
        Some(b's') => (&value[..len - 1], 1),     // seconds (allowed for completeness)
        _ => (value, 1),
    };
    num.parse::<i64>()?
        .checked_mul(factor)
        .ok_or_else(|| Error::SdpInvalidValue(value.to_owned()))
}
