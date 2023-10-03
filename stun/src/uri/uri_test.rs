use super::*;

#[test]
fn test_parse_uri() -> Result<()> {
    let tests = vec![
        (
            "default",
            "stun:example.org",
            Uri {
                host: "example.org".to_owned(),
                scheme: SCHEME.to_owned(),
                port: None,
            },
            "stun:example.org",
        ),
        (
            "secure",
            "stuns:example.org",
            Uri {
                host: "example.org".to_owned(),
                scheme: SCHEME_SECURE.to_owned(),
                port: None,
            },
            "stuns:example.org",
        ),
        (
            "with port",
            "stun:example.org:8000",
            Uri {
                host: "example.org".to_owned(),
                scheme: SCHEME.to_owned(),
                port: Some(8000),
            },
            "stun:example.org:8000",
        ),
        (
            "ipv6 address",
            "stun:[::1]:123",
            Uri {
                host: "::1".to_owned(),
                scheme: SCHEME.to_owned(),
                port: Some(123),
            },
            "stun:[::1]:123",
        ),
    ];

    for (name, input, output, expected_str) in tests {
        let out = Uri::parse_uri(input)?;
        assert_eq!(out, output, "{name}: {out} != {output}");
        assert_eq!(out.to_string(), expected_str, "{name}");
    }

    //"MustFail"
    {
        let tests = vec![
            ("hierarchical", "stun://example.org"),
            ("bad scheme", "tcp:example.org"),
            ("invalid uri scheme", "stun_s:test"),
        ];
        for (name, input) in tests {
            let result = Uri::parse_uri(input);
            assert!(result.is_err(), "{name} should fail, but did not");
        }
    }

    Ok(())
}
