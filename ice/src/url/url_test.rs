use super::*;

#[test]
fn test_parse_url_success() -> Result<()> {
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

        assert_eq!(url.scheme, expected_scheme, "testCase: {raw_url:?}");
        assert_eq!(
            expected_url_string,
            url.to_string(),
            "testCase: {raw_url:?}"
        );
        assert_eq!(url.is_secure(), expected_secure, "testCase: {raw_url:?}");
        assert_eq!(url.host, expected_host, "testCase: {raw_url:?}");
        assert_eq!(url.port, expected_port, "testCase: {raw_url:?}");
        assert_eq!(url.proto, expected_proto, "testCase: {raw_url:?}");
    }

    Ok(())
}

#[test]
fn test_parse_url_failure() -> Result<()> {
    let tests = vec![
        ("", Error::ErrSchemeType),
        (":::", Error::ErrUrlParse),
        ("stun:[::1]:123:", Error::ErrPort),
        ("stun:[::1]:123a", Error::ErrPort),
        ("google.de", Error::ErrSchemeType),
        ("stun:", Error::ErrHost),
        ("stun:google.de:abc", Error::ErrPort),
        ("stun:google.de?transport=udp", Error::ErrStunQuery),
        ("stuns:google.de?transport=udp", Error::ErrStunQuery),
        ("turn:google.de?trans=udp", Error::ErrInvalidQuery),
        ("turns:google.de?trans=udp", Error::ErrInvalidQuery),
        (
            "turns:google.de?transport=udp&another=1",
            Error::ErrInvalidQuery,
        ),
        ("turn:google.de?transport=ip", Error::ErrProtoType),
    ];

    for (raw_url, expected_err) in tests {
        let result = Url::parse_url(raw_url);
        if let Err(err) = result {
            assert_eq!(
                err.to_string(),
                expected_err.to_string(),
                "testCase: '{raw_url}', expected err '{expected_err}', but got err '{err}'"
            );
        } else {
            panic!("expected error, but got ok");
        }
    }

    Ok(())
}
