use std::collections::HashMap;

type Fmtp = HashMap<String, String>;

/// parse_fmtp parses fmtp string.
pub(crate) fn parse_fmtp(line: &str) -> Fmtp {
    let mut f = Fmtp::new();
    for p in line.split(';').collect::<Vec<&str>>() {
        let pp: Vec<&str> = p.trim().splitn(2, '=').collect();
        let key = pp[0].to_lowercase();
        let value = if pp.len() > 1 {
            pp[1].to_owned()
        } else {
            String::new()
        };
        f.insert(key, value);
    }
    f
}

/// fmtp_consist checks that two FMTP parameters are not inconsistent.
pub(crate) fn fmtp_consist(a: &Fmtp, b: &Fmtp) -> bool {
    //TODO: add unicode case-folding equal support
    for (k, v) in a {
        if let Some(vb) = b.get(k) {
            if vb.to_uppercase() != v.to_uppercase() {
                return false;
            }
        }
    }
    for (k, v) in b {
        if let Some(va) = a.get(k) {
            if va.to_uppercase() != v.to_uppercase() {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_fmtp() {
        let tests: Vec<(&str, &str, Fmtp)> = vec![
            (
                "OneParam",
                "key-name=value",
                [("key-name".to_owned(), "value".to_owned())]
                    .iter()
                    .cloned()
                    .collect(),
            ),
            (
                "OneParamWithWhiteSpeces",
                "\tkey-name=value ",
                [("key-name".to_owned(), "value".to_owned())]
                    .iter()
                    .cloned()
                    .collect(),
            ),
            (
                "TwoParams",
                "key-name=value;key2=value2",
                [
                    ("key-name".to_owned(), "value".to_owned()),
                    ("key2".to_owned(), "value2".to_owned()),
                ]
                .iter()
                .cloned()
                .collect(),
            ),
            (
                "TwoParamsWithWhiteSpeces",
                "key-name=value;  \n\tkey2=value2 ",
                [
                    ("key-name".to_owned(), "value".to_owned()),
                    ("key2".to_owned(), "value2".to_owned()),
                ]
                .iter()
                .cloned()
                .collect(),
            ),
        ];

        for (name, input, expected) in tests {
            let f = parse_fmtp(input);
            assert_eq!(
                expected, f,
                "{} Expected Fmtp params: {:?}, got: {:?}",
                name, expected, f
            );
        }
    }

    #[test]
    fn test_fmtp_consist() {
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
                let c = fmtp_consist(&parse_fmtp(a), &parse_fmtp(b));
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
            check(b, a);
        }
    }
}
