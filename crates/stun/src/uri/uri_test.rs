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
        ),
        (
            "secure",
            "stuns:example.org",
            URI {
                host: "example.org".to_owned(),
                scheme: SCHEME_SECURE.to_owned(),
                port: None,
            },
        ),
        (
            "with port",
            "stun:example.org:8000",
            URI {
                host: "example.org".to_owned(),
                scheme: SCHEME.to_owned(),
                port: Some(8000),
            },
        ),
    ];

    for (name, input, output) in tests {
        let out = URI::parse_uri(input.to_owned())?;
        assert_eq!(out, output, "{}: {} != {}", name, out, output);
    }

    //"MustFail"
    {
        let tests = vec![
            ("hierarchical", "stun://example.org"),
            ("bad scheme", "tcp:example.org"),
            ("invalid uri scheme", "stun_s:test"),
        ];
        for (name, input) in tests {
            let result = URI::parse_uri(input.to_owned());
            assert!(result.is_err(), "{} should fail, but did not", name);
        }
    }

    Ok(())
}
