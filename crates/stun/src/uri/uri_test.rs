use super::*;

use util::Error;

#[test]
fn test_parse_uri() -> Result<(), Error> {
    let tests = vec![
        (
            "default",
            "stun:example.org",
            URI {
                host: "example.org".to_owned(),
                scheme: SCHEME.to_owned(),
                port: None,
            },
            "stun:example.org",
        ),
        (
            "secure",
            "stuns:example.org",
            URI {
                host: "example.org".to_owned(),
                scheme: SCHEME_SECURE.to_owned(),
                port: None,
            },
            "stuns:example.org",
        ),
        (
            "with port",
            "stun:example.org:8000",
            URI {
                host: "example.org".to_owned(),
                scheme: SCHEME.to_owned(),
                port: Some(8000),
            },
            "stun:example.org:8000",
        ),
        (
            "ipv6 address",
            "stun:[::1]:123",
            URI {
                host: "::1".to_owned(),
                scheme: SCHEME.to_owned(),
                port: Some(123),
            },
            "stun:[::1]:123",
        ),
    ];

    for (name, input, output, expected_str) in tests {
        let out = URI::parse_uri(input)?;
        assert_eq!(out, output, "{}: {} != {}", name, out, output);
        assert_eq!(out.to_string(), expected_str, "{}", name);
    }

    //"MustFail"
    {
        let tests = vec![
            ("hierarchical", "stun://example.org"),
            ("bad scheme", "tcp:example.org"),
            ("invalid uri scheme", "stun_s:test"),
        ];
        for (name, input) in tests {
            let result = URI::parse_uri(input);
            assert!(result.is_err(), "{} should fail, but did not", name);
        }
    }

    Ok(())
}
