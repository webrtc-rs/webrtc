use super::*;

#[test]
fn test_parse_url_success() -> Result<(), Error> {
    let tests = vec![
        (
            "stun:google.de",
            "stun:google.de:3478",
            SchemeType::Stun,
            false,
            "google.de",
            3478,
            ProtoType::Udp,
        ),
        (
            "stun:google.de:1234",
            "stun:google.de:1234",
            SchemeType::Stun,
            false,
            "google.de",
            1234,
            ProtoType::Udp,
        ),
        (
            "stuns:google.de",
            "stuns:google.de:5349",
            SchemeType::Stuns,
            true,
            "google.de",
            5349,
            ProtoType::Tcp,
        ),
        (
            "stun:[::1]:123",
            "stun:[::1]:123",
            SchemeType::Stun,
            false,
            "::1",
            123,
            ProtoType::Udp,
        ),
        (
            "turn:google.de",
            "turn:google.de:3478?transport=udp",
            SchemeType::Turn,
            false,
            "google.de",
            3478,
            ProtoType::Udp,
        ),
        (
            "turns:google.de",
            "turns:google.de:5349?transport=tcp",
            SchemeType::Turns,
            true,
            "google.de",
            5349,
            ProtoType::Tcp,
        ),
        (
            "turn:google.de?transport=udp",
            "turn:google.de:3478?transport=udp",
            SchemeType::Turn,
            false,
            "google.de",
            3478,
            ProtoType::Udp,
        ),
        (
            "turns:google.de?transport=tcp",
            "turns:google.de:5349?transport=tcp",
            SchemeType::Turns,
            true,
            "google.de",
            5349,
            ProtoType::Tcp,
        ),
    ];

    for (
        raw_url,
        expected_url_string,
        expected_scheme,
        expected_secure,
        expected_host,
        expected_port,
        expected_proto,
    ) in tests
    {
        let url = Url::parse_url(raw_url)?;

        assert_eq!(expected_scheme, url.scheme, "testCase: {:?}", raw_url);
        assert_eq!(
            expected_url_string,
            url.to_string(),
            "testCase: {:?}",
            raw_url
        );
        assert_eq!(expected_secure, url.is_secure(), "testCase: {:?}", raw_url);
        assert_eq!(expected_host, url.host, "testCase: {:?}", raw_url);
        assert_eq!(expected_port, url.port, "testCase: {:?}", raw_url);
        assert_eq!(expected_proto, url.proto, "testCase: {:?}", raw_url);
    }

    Ok(())
}

#[test]
fn test_parse_url_failure() -> Result<(), Error> {
    let tests = vec![
        ("", ERR_SCHEME_TYPE.to_owned()),
        (":::", ERR_URL_PARSE_ERROR.to_owned()),
        ("stun:[::1]:123:", ERR_PORT.to_owned()),
        ("stun:[::1]:123a", ERR_PORT.to_owned()),
        ("google.de", ERR_SCHEME_TYPE.to_owned()),
        ("stun:", ERR_HOST.to_owned()),
        ("stun:google.de:abc", ERR_PORT.to_owned()),
        ("stun:google.de?transport=udp", ERR_STUN_QUERY.to_owned()),
        ("stuns:google.de?transport=udp", ERR_STUN_QUERY.to_owned()),
        ("turn:google.de?trans=udp", ERR_INVALID_QUERY.to_owned()),
        ("turns:google.de?trans=udp", ERR_INVALID_QUERY.to_owned()),
        (
            "turns:google.de?transport=udp&another=1",
            ERR_INVALID_QUERY.to_owned(),
        ),
        ("turn:google.de?transport=ip", ERR_PROTO_TYPE.to_owned()),
    ];

    for (raw_url, expected_err) in tests {
        let result = Url::parse_url(raw_url);
        if let Err(err) = result {
            assert_eq!(err, expected_err, "testCase:{}", raw_url);
        } else {
            panic!("expected error, but got ok");
        }
    }

    Ok(())
}
