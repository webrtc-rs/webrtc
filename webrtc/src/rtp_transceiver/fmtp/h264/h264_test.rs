use super::*;

#[test]
fn test_h264_fmtp_parse() {
    let tests: Vec<(&str, &str, Box<dyn Fmtp>)> = vec![
        (
            "OneParam",
            "key-name=value",
            Box::new(H264Fmtp {
                parameters: [("key-name".to_owned(), "value".to_owned())]
                    .iter()
                    .cloned()
                    .collect(),
            }),
        ),
        (
            "OneParamWithWhiteSpeces",
            "\tkey-name=value ",
            Box::new(H264Fmtp {
                parameters: [("key-name".to_owned(), "value".to_owned())]
                    .iter()
                    .cloned()
                    .collect(),
            }),
        ),
        (
            "TwoParams",
            "key-name=value;key2=value2",
            Box::new(H264Fmtp {
                parameters: [
                    ("key-name".to_owned(), "value".to_owned()),
                    ("key2".to_owned(), "value2".to_owned()),
                ]
                .iter()
                .cloned()
                .collect(),
            }),
        ),
        (
            "TwoParamsWithWhiteSpeces",
            "key-name=value;  \n\tkey2=value2 ",
            Box::new(H264Fmtp {
                parameters: [
                    ("key-name".to_owned(), "value".to_owned()),
                    ("key2".to_owned(), "value2".to_owned()),
                ]
                .iter()
                .cloned()
                .collect(),
            }),
        ),
    ];

    for (name, input, expected) in tests {
        let f = parse("video/h264", input);
        assert_eq!(&f, &expected, "{name} failed");

        assert_eq!(f.mime_type(), "video/h264");
    }
}

#[test]
fn test_h264_fmtp_compare() {
    let consist_string: HashMap<bool, String> = [
        (true, "consist".to_owned()),
        (false, "inconsist".to_owned()),
    ]
    .iter()
    .cloned()
    .collect();

    let tests = vec![
        (
            "Equal",
            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f",
            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f",
            true,
        ),
        (
            "EqualWithWhitespaceVariants",
            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f",
            "  level-asymmetry-allowed=1;  \npacketization-mode=1;\t\nprofile-level-id=42e01f",
            true,
        ),
        (
            "EqualWithCase",
            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f",
            "level-asymmetry-allowed=1;packetization-mode=1;PROFILE-LEVEL-ID=42e01f",
            true,
        ),
        (
            "OneHasExtraParam",
            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f",
            "packetization-mode=1;profile-level-id=42e01f",
            true,
        ),
        (
            "DifferentProfileLevelIDVersions",
            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f",
            "packetization-mode=1;profile-level-id=42e029",
            true,
        ),
        (
            "Inconsistent",
            "packetization-mode=1;profile-level-id=42e029",
            "packetization-mode=0;profile-level-id=42e029",
            false,
        ),
        (
            "Inconsistent_MissingPacketizationMode",
            "packetization-mode=1;profile-level-id=42e029",
            "profile-level-id=42e029",
            false,
        ),
        (
            "Inconsistent_MissingProfileLevelID",
            "packetization-mode=1;profile-level-id=42e029",
            "packetization-mode=1",
            false,
        ),
        (
            "Inconsistent_InvalidProfileLevelID",
            "packetization-mode=1;profile-level-id=42e029",
            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=41e029",
            false,
        ),
    ];

    for (name, a, b, consist) in tests {
        let check = |a, b| {
            let aa = parse("video/h264", a);
            let bb = parse("video/h264", b);

            // test forward case here
            let c = aa.match_fmtp(&*bb);
            assert_eq!(
                c,
                consist,
                "{}: '{}' and '{}' are expected to be {:?}, but treated as {:?}",
                name,
                a,
                b,
                consist_string.get(&consist),
                consist_string.get(&c),
            );

            // test reverse case here
            let c = bb.match_fmtp(&*aa);
            assert_eq!(
                c,
                consist,
                "{}: '{}' and '{}' are expected to be {:?}, but treated as {:?}",
                name,
                a,
                b,
                consist_string.get(&consist),
                consist_string.get(&c),
            );
        };

        check(a, b);
    }
}
