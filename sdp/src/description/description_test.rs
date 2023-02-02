use super::common::*;
use super::media::*;
use super::session::*;
use crate::error::{Error, Result};

use std::io::Cursor;
use url::Url;

const CANONICAL_MARSHAL_SDP: &str = "v=0\r\n\
     o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
     s=SDP Seminar\r\n\
     i=A Seminar on the session description protocol\r\n\
     u=http://www.example.com/seminars/sdp.pdf\r\n\
     e=j.doe@example.com (Jane Doe)\r\n\
     p=+1 617 555-6011\r\n\
     c=IN IP4 224.2.17.12/127\r\n\
     b=X-YZ:128\r\n\
     b=AS:12345\r\n\
     t=2873397496 2873404696\r\n\
     t=3034423619 3042462419\r\n\
     r=604800 3600 0 90000\r\n\
     z=2882844526 -3600 2898848070 0\r\n\
     k=prompt\r\n\
     a=candidate:0 1 UDP 2113667327 203.0.113.1 54400 typ host\r\n\
     a=recvonly\r\n\
     m=audio 49170 RTP/AVP 0\r\n\
     i=Vivamus a posuere nisl\r\n\
     c=IN IP4 203.0.113.1\r\n\
     b=X-YZ:128\r\n\
     k=prompt\r\n\
     a=sendrecv\r\n\
     m=video 51372 RTP/AVP 99\r\n\
     a=rtpmap:99 h263-1998/90000\r\n";

#[test]
fn test_unmarshal_marshal() -> Result<()> {
    let input = CANONICAL_MARSHAL_SDP;
    let mut reader = Cursor::new(input.as_bytes());
    let sdp = SessionDescription::unmarshal(&mut reader)?;
    let output = sdp.marshal();
    assert_eq!(output, input);

    Ok(())
}

#[test]
fn test_marshal() -> Result<()> {
    let sd = SessionDescription {
        version: 0,
        origin: Origin {
            username: "jdoe".to_string(),
            session_id: 2890844526,
            session_version: 2890842807,
            network_type: "IN".to_string(),
            address_type: "IP4".to_string(),
            unicast_address: "10.47.16.5".to_string(),
        },
        session_name: "SDP Seminar".to_string(),
        session_information: Some("A Seminar on the session description protocol".to_string()),
        uri: Some(Url::parse("http://www.example.com/seminars/sdp.pdf")?),
        email_address: Some("j.doe@example.com (Jane Doe)".to_string()),
        phone_number: Some("+1 617 555-6011".to_string()),
        connection_information: Some(ConnectionInformation {
            network_type: "IN".to_string(),
            address_type: "IP4".to_string(),
            address: Some(Address {
                address: "224.2.17.12".to_string(),
                ttl: Some(127),
                range: None,
            }),
        }),
        bandwidth: vec![
            Bandwidth {
                experimental: true,
                bandwidth_type: "YZ".to_string(),
                bandwidth: 128,
            },
            Bandwidth {
                experimental: false,
                bandwidth_type: "AS".to_string(),
                bandwidth: 12345,
            },
        ],
        time_descriptions: vec![
            TimeDescription {
                timing: Timing {
                    start_time: 2873397496,
                    stop_time: 2873404696,
                },
                repeat_times: vec![],
            },
            TimeDescription {
                timing: Timing {
                    start_time: 3034423619,
                    stop_time: 3042462419,
                },
                repeat_times: vec![RepeatTime {
                    interval: 604800,
                    duration: 3600,
                    offsets: vec![0, 90000],
                }],
            },
        ],
        time_zones: vec![
            TimeZone {
                adjustment_time: 2882844526,
                offset: -3600,
            },
            TimeZone {
                adjustment_time: 2898848070,
                offset: 0,
            },
        ],
        encryption_key: Some("prompt".to_string()),
        attributes: vec![
            Attribute::new(
                "candidate".to_string(),
                Some("0 1 UDP 2113667327 203.0.113.1 54400 typ host".to_string()),
            ),
            Attribute::new("recvonly".to_string(), None),
        ],
        media_descriptions: vec![
            MediaDescription {
                media_name: MediaName {
                    media: "audio".to_string(),
                    port: RangedPort {
                        value: 49170,
                        range: None,
                    },
                    protos: vec!["RTP".to_string(), "AVP".to_string()],
                    formats: vec!["0".to_string()],
                },
                media_title: Some("Vivamus a posuere nisl".to_string()),
                connection_information: Some(ConnectionInformation {
                    network_type: "IN".to_string(),
                    address_type: "IP4".to_string(),
                    address: Some(Address {
                        address: "203.0.113.1".to_string(),
                        ttl: None,
                        range: None,
                    }),
                }),
                bandwidth: vec![Bandwidth {
                    experimental: true,
                    bandwidth_type: "YZ".to_string(),
                    bandwidth: 128,
                }],
                encryption_key: Some("prompt".to_string()),
                attributes: vec![Attribute::new("sendrecv".to_string(), None)],
            },
            MediaDescription {
                media_name: MediaName {
                    media: "video".to_string(),
                    port: RangedPort {
                        value: 51372,
                        range: None,
                    },
                    protos: vec!["RTP".to_string(), "AVP".to_string()],
                    formats: vec!["99".to_string()],
                },
                media_title: None,
                connection_information: None,
                bandwidth: vec![],
                encryption_key: None,
                attributes: vec![Attribute::new(
                    "rtpmap".to_string(),
                    Some("99 h263-1998/90000".to_string()),
                )],
            },
        ],
    };

    let actual = sd.marshal();
    assert!(
        actual == CANONICAL_MARSHAL_SDP,
        "error:\n\nEXPECTED:\n{CANONICAL_MARSHAL_SDP}\nACTUAL:\n{actual}!!!!\n"
    );

    Ok(())
}

const BASE_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n";

const SESSION_INFORMATION_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
i=A Seminar on the session description protocol\r\n\
t=3034423619 3042462419\r\n";

// https://tools.ietf.org/html/rfc4566#section-5
// Parsers SHOULD be tolerant and also accept records terminated
// with a single newline character.
const SESSION_INFORMATION_SDPLFONLY: &str = "v=0\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\n\
s=SDP Seminar\n\
i=A Seminar on the session description protocol\n\
t=3034423619 3042462419\n";

// SessionInformationSDPCROnly = "v=0\r" +
// 	"o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r" +
// 	"s=SDP Seminar\r"
// 	"i=A Seminar on the session description protocol\r" +
// 	"t=3034423619 3042462419\r"

// Other SDP parsers (e.g. one in VLC media player) allow
// empty lines.
const SESSION_INFORMATION_SDPEXTRA_CRLF: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
\r\n\
s=SDP Seminar\r\n\
\r\n\
i=A Seminar on the session description protocol\r\n\
\r\n\
t=3034423619 3042462419\r\n\
\r\n";

const URI_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
u=http://www.example.com/seminars/sdp.pdf\r\n\
t=3034423619 3042462419\r\n";

const EMAIL_ADDRESS_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
e=j.doe@example.com (Jane Doe)\r\n\
t=3034423619 3042462419\r\n";

const PHONE_NUMBER_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
p=+1 617 555-6011\r\n\
t=3034423619 3042462419\r\n";

const SESSION_CONNECTION_INFORMATION_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
c=IN IP4 224.2.17.12/127\r\n\
t=3034423619 3042462419\r\n";

const SESSION_BANDWIDTH_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
b=X-YZ:128\r\n\
b=AS:12345\r\n\
t=3034423619 3042462419\r\n";

const TIMING_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n";

// Short hand time notation is converted into NTP timestamp format in
// seconds. Because of that unittest comparisons will fail as the same time
// will be expressed in different units.
const REPEAT_TIMES_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
r=604800 3600 0 90000\r\n\
r=3d 2h 0 21h\r\n";

const REPEAT_TIMES_SDPEXPECTED: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
r=604800 3600 0 90000\r\n\
r=259200 7200 0 75600\r\n";

const REPEAT_TIMES_OVERFLOW_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
r=604800 3600 0 90000\r\n\
r=106751991167301d 2h 0 21h\r\n";

const REPEAT_TIMES_SDPEXTRA_CRLF: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
r=604800 3600 0 90000\r\n\
r=259200 7200 0 75600\r\n\
\r\n";

// The expected value looks a bit different for the same reason as mentioned
// above regarding RepeatTimes.
const TIME_ZONES_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
r=2882844526 -1h 2898848070 0\r\n";

const TIME_ZONES_SDPEXPECTED: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
r=2882844526 -3600 2898848070 0\r\n";

const TIME_ZONES_SDP2: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
z=2882844526 -3600 2898848070 0\r\n";

const TIME_ZONES_SDP2EXTRA_CRLF: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
z=2882844526 -3600 2898848070 0\r\n\
\r\n";

const SESSION_ENCRYPTION_KEY_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
k=prompt\r\n";

const SESSION_ENCRYPTION_KEY_SDPEXTRA_CRLF: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
k=prompt\r\n
\r\n";

const SESSION_ATTRIBUTES_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
a=rtpmap:96 opus/48000\r\n";

const MEDIA_NAME_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
m=video 51372 RTP/AVP 99\r\n\
m=audio 54400 RTP/SAVPF 0 96\r\n";

const MEDIA_NAME_SDPEXTRA_CRLF: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
m=video 51372 RTP/AVP 99\r\n\
m=audio 54400 RTP/SAVPF 0 96\r\n
\r\n";

const MEDIA_TITLE_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
m=video 51372 RTP/AVP 99\r\n\
m=audio 54400 RTP/SAVPF 0 96\r\n\
i=Vivamus a posuere nisl\r\n";

const MEDIA_CONNECTION_INFORMATION_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
m=video 51372 RTP/AVP 99\r\n\
m=audio 54400 RTP/SAVPF 0 96\r\n\
c=IN IP4 203.0.113.1\r\n";

const MEDIA_CONNECTION_INFORMATION_SDPEXTRA_CRLF: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
m=video 51372 RTP/AVP 99\r\n\
m=audio 54400 RTP/SAVPF 0 96\r\n\
c=IN IP4 203.0.113.1\r\n\
\r\n";

const MEDIA_DESCRIPTION_OUT_OF_ORDER_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
m=video 51372 RTP/AVP 99\r\n\
m=audio 54400 RTP/SAVPF 0 96\r\n\
a=rtpmap:99 h263-1998/90000\r\n\
a=candidate:0 1 UDP 2113667327 203.0.113.1 54400 typ host\r\n\
c=IN IP4 203.0.113.1\r\n\
i=Vivamus a posuere nisl\r\n";

const MEDIA_DESCRIPTION_OUT_OF_ORDER_SDPACTUAL: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
m=video 51372 RTP/AVP 99\r\n\
m=audio 54400 RTP/SAVPF 0 96\r\n\
i=Vivamus a posuere nisl\r\n\
c=IN IP4 203.0.113.1\r\n\
a=rtpmap:99 h263-1998/90000\r\n\
a=candidate:0 1 UDP 2113667327 203.0.113.1 54400 typ host\r\n";

const MEDIA_BANDWIDTH_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
m=video 51372 RTP/AVP 99\r\n\
m=audio 54400 RTP/SAVPF 0 96\r\n\
b=X-YZ:128\r\n\
b=AS:12345\r\n";

const MEDIA_ENCRYPTION_KEY_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
m=video 51372 RTP/AVP 99\r\n\
m=audio 54400 RTP/SAVPF 0 96\r\n\
k=prompt\r\n";

const MEDIA_ENCRYPTION_KEY_SDPEXTRA_CRLF: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
m=video 51372 RTP/AVP 99\r\n\
m=audio 54400 RTP/SAVPF 0 96\r\n\
k=prompt\r\n\
\r\n";

const MEDIA_ATTRIBUTES_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
t=2873397496 2873404696\r\n\
m=video 51372 RTP/AVP 99\r\n\
m=audio 54400 RTP/SAVPF 0 96\r\n\
a=rtpmap:99 h263-1998/90000\r\n\
a=candidate:0 1 UDP 2113667327 203.0.113.1 54400 typ host\r\n\
a=rtcp-fb:97 ccm fir\r\n\
a=rtcp-fb:97 nack\r\n\
a=rtcp-fb:97 nack pli\r\n";

const CANONICAL_UNMARSHAL_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
i=A Seminar on the session description protocol\r\n\
u=http://www.example.com/seminars/sdp.pdf\r\n\
e=j.doe@example.com (Jane Doe)\r\n\
p=+1 617 555-6011\r\n\
c=IN IP4 224.2.17.12/127\r\n\
b=X-YZ:128\r\n\
b=AS:12345\r\n\
t=2873397496 2873404696\r\n\
t=3034423619 3042462419\r\n\
r=604800 3600 0 90000\r\n\
z=2882844526 -3600 2898848070 0\r\n\
k=prompt\r\n\
a=candidate:0 1 UDP 2113667327 203.0.113.1 54400 typ host\r\n\
a=recvonly\r\n\
m=audio 49170 RTP/AVP 0\r\n\
i=Vivamus a posuere nisl\r\n\
c=IN IP4 203.0.113.1\r\n\
b=X-YZ:128\r\n\
k=prompt\r\n\
a=sendrecv\r\n\
m=video 51372 RTP/AVP 99\r\n\
a=rtpmap:99 h263-1998/90000\r\n";

#[test]
fn test_round_trip() -> Result<()> {
    let tests = vec![
        (
            "SessionInformationSDPLFOnly",
            SESSION_INFORMATION_SDPLFONLY,
            Some(SESSION_INFORMATION_SDP),
        ),
        (
            "SessionInformationSDPExtraCRLF",
            SESSION_INFORMATION_SDPEXTRA_CRLF,
            Some(SESSION_INFORMATION_SDP),
        ),
        ("SessionInformation", SESSION_INFORMATION_SDP, None),
        ("URI", URI_SDP, None),
        ("EmailAddress", EMAIL_ADDRESS_SDP, None),
        ("PhoneNumber", PHONE_NUMBER_SDP, None),
        (
            "RepeatTimesSDPExtraCRLF",
            REPEAT_TIMES_SDPEXTRA_CRLF,
            Some(REPEAT_TIMES_SDPEXPECTED),
        ),
        (
            "SessionConnectionInformation",
            SESSION_CONNECTION_INFORMATION_SDP,
            None,
        ),
        ("SessionBandwidth", SESSION_BANDWIDTH_SDP, None),
        ("SessionEncryptionKey", SESSION_ENCRYPTION_KEY_SDP, None),
        (
            "SessionEncryptionKeyExtraCRLF",
            SESSION_ENCRYPTION_KEY_SDPEXTRA_CRLF,
            Some(SESSION_ENCRYPTION_KEY_SDP),
        ),
        ("SessionAttributes", SESSION_ATTRIBUTES_SDP, None),
        (
            "TimeZonesSDP2ExtraCRLF",
            TIME_ZONES_SDP2EXTRA_CRLF,
            Some(TIME_ZONES_SDP2),
        ),
        ("MediaName", MEDIA_NAME_SDP, None),
        (
            "MediaNameExtraCRLF",
            MEDIA_NAME_SDPEXTRA_CRLF,
            Some(MEDIA_NAME_SDP),
        ),
        ("MediaTitle", MEDIA_TITLE_SDP, None),
        (
            "MediaConnectionInformation",
            MEDIA_CONNECTION_INFORMATION_SDP,
            None,
        ),
        (
            "MediaConnectionInformationExtraCRLF",
            MEDIA_CONNECTION_INFORMATION_SDPEXTRA_CRLF,
            Some(MEDIA_CONNECTION_INFORMATION_SDP),
        ),
        (
            "MediaDescriptionOutOfOrder",
            MEDIA_DESCRIPTION_OUT_OF_ORDER_SDP,
            Some(MEDIA_DESCRIPTION_OUT_OF_ORDER_SDPACTUAL),
        ),
        ("MediaBandwidth", MEDIA_BANDWIDTH_SDP, None),
        ("MediaEncryptionKey", MEDIA_ENCRYPTION_KEY_SDP, None),
        (
            "MediaEncryptionKeyExtraCRLF",
            MEDIA_ENCRYPTION_KEY_SDPEXTRA_CRLF,
            Some(MEDIA_ENCRYPTION_KEY_SDP),
        ),
        ("MediaAttributes", MEDIA_ATTRIBUTES_SDP, None),
        ("CanonicalUnmarshal", CANONICAL_UNMARSHAL_SDP, None),
    ];

    for (name, sdp_str, expected) in tests {
        let mut reader = Cursor::new(sdp_str.as_bytes());
        let sdp = SessionDescription::unmarshal(&mut reader);
        if let Ok(sdp) = sdp {
            let actual = sdp.marshal();
            if let Some(expected) = expected {
                assert_eq!(actual.as_str(), expected, "{name}\n{sdp_str}");
            } else {
                assert_eq!(actual.as_str(), sdp_str, "{name}\n{sdp_str}");
            }
        } else {
            panic!("{name}\n{sdp_str}");
        }
    }

    Ok(())
}

#[test]
fn test_unmarshal_repeat_times() -> Result<()> {
    let mut reader = Cursor::new(REPEAT_TIMES_SDP.as_bytes());
    let sdp = SessionDescription::unmarshal(&mut reader)?;
    let actual = sdp.marshal();
    assert_eq!(actual.as_str(), REPEAT_TIMES_SDPEXPECTED);
    Ok(())
}

#[test]
fn test_unmarshal_repeat_times_overflow() -> Result<()> {
    let mut reader = Cursor::new(REPEAT_TIMES_OVERFLOW_SDP.as_bytes());
    let result = SessionDescription::unmarshal(&mut reader);
    assert!(result.is_err());
    assert_eq!(
        Error::SdpInvalidValue("106751991167301d".to_owned()),
        result.unwrap_err()
    );
    Ok(())
}

#[test]
fn test_unmarshal_time_zones() -> Result<()> {
    let mut reader = Cursor::new(TIME_ZONES_SDP.as_bytes());
    let sdp = SessionDescription::unmarshal(&mut reader)?;
    let actual = sdp.marshal();
    assert_eq!(actual.as_str(), TIME_ZONES_SDPEXPECTED);
    Ok(())
}

#[test]
fn test_unmarshal_non_nil_address() -> Result<()> {
    let input = "v=0\r\no=0 0 0 IN IP4 0\r\ns=0\r\nc=IN IP4\r\nt=0 0\r\n";
    let mut reader = Cursor::new(input);
    let sdp = SessionDescription::unmarshal(&mut reader);
    if let Ok(sdp) = sdp {
        let output = sdp.marshal();
        assert_eq!(output.as_str(), input);
    } else {
        panic!("{}", input);
    }
    Ok(())
}
