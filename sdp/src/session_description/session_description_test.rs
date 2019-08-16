use super::*;

use utils::Error;

use std::io::BufReader;

const CanonicalMarshalSDP: &'static str =
    "v=0\r\n\
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
fn test_unmarshal_marshal() -> Result<(), Error> {
    let input = CanonicalMarshalSDP;
    let mut reader = BufReader::new(input.as_bytes());
    let sdp = SessionDescription::unmarshal(&mut reader)?;
    let output = sdp.marshal();
    assert_eq!(output, input);

    Ok(())
}

#[test]
fn test_marshal() -> Result<(), Error> {
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
        &actual == CanonicalMarshalSDP,
        "error:\n\nEXPECTED:\n{}\nACTUAL:\n{}!!!!\n",
        CanonicalMarshalSDP,
        actual
    );

    Ok(())
}
