use super::*;

#[test]
fn test_generic_fmtp_parse() {
    let tests: Vec<(&str, &str, Box<dyn Fmtp>)> = vec![
        (
            "OneParam",
            "key-name=value",
            Box::new(GenericFmtp {
                mime_type: "generic".to_owned(),
                parameters: [("key-name".to_owned(), "value".to_owned())]
                    .iter()
                    .cloned()
                    .collect(),
            }),
        ),
        (
            "OneParamWithWhiteSpeces",
            "\tkey-name=value ",
            Box::new(GenericFmtp {
                mime_type: "generic".to_owned(),
                parameters: [("key-name".to_owned(), "value".to_owned())]
                    .iter()
                    .cloned()
                    .collect(),
            }),
        ),
        (
            "TwoParams",
            "key-name=value;key2=value2",
            Box::new(GenericFmtp {
                mime_type: "generic".to_owned(),
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
            Box::new(GenericFmtp {
                mime_type: "generic".to_owned(),
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
        let f = parse("generic", input);
        assert_eq!(&f, &expected, "{name} failed");

        assert_eq!(f.mime_type(), "generic");
    }
}

#[test]
fn test_generic_fmtp_compare() {
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
            "key1=value1;key2=value2;key3=value3",
            "key1=value1;key2=value2;key3=value3",
            true,
        ),
        (
            "EqualWithWhitespaceVariants",
            "key1=value1;key2=value2;key3=value3",
            "  key1=value1;  \nkey2=value2;\t\nkey3=value3",
            true,
        ),
        (
            "EqualWithCase",
            "key1=value1;key2=value2;key3=value3",
            "key1=value1;key2=Value2;Key3=value3",
            true,
        ),
        (
            "OneHasExtraParam",
            "key1=value1;key2=value2;key3=value3",
            "key1=value1;key2=value2;key3=value3;key4=value4",
            true,
        ),
        (
            "Inconsistent",
            "key1=value1;key2=value2;key3=value3",
            "key1=value1;key2=different_value;key3=value3",
            false,
        ),
        (
            "Inconsistent_OneHasExtraParam",
            "key1=value1;key2=value2;key3=value3;key4=value4",
            "key1=value1;key2=different_value;key3=value3",
            false,
        ),
    ];

    for (name, a, b, consist) in tests {
        let check = |a, b| {
            let aa = parse("", a);
            let bb = parse("", b);

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

#[test]
fn test_generic_fmtp_compare_mime_type_case_mismatch() {
    let a = parse("video/vp8", "");
    let b = parse("video/VP8", "");

    assert!(
        b.match_fmtp(&*a),
        "fmtp lines should match even if they use different casing"
    );
}
