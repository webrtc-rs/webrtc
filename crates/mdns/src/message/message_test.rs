use super::name::*;
use crate::errors::*;

use std::collections::HashMap;
use util::Error;

#[test]
fn test_name() -> Result<(), Error> {
    let tests = vec![
        "",
        ".",
        "google..com",
        "google.com",
        "google..com.",
        "google.com.",
        ".google.com.",
        "www..google.com.",
        "www.google.com.",
    ];

    for test in tests {
        let name = Name::new(test.to_owned())?;
        let ns = name.to_string();
        assert_eq!(ns, test, "got {} = {}, want = {}", name, ns, test);
    }

    Ok(())
}

#[test]
fn test_name_pack_unpack() -> Result<(), Error> {
    let tests = vec![
        ("", "", Some(ERR_NON_CANONICAL_NAME.to_owned())),
        (".", ".", None),
        ("google..com", "", Some(ERR_NON_CANONICAL_NAME.to_owned())),
        ("google.com", "", Some(ERR_NON_CANONICAL_NAME.to_owned())),
        ("google..com.", "", Some(ERR_ZERO_SEG_LEN.to_owned())),
        ("google.com.", "google.com.", None),
        (".google.com.", "", Some(ERR_ZERO_SEG_LEN.to_owned())),
        ("www..google.com.", "", Some(ERR_ZERO_SEG_LEN.to_owned())),
        ("www.google.com.", "www.google.com.", None),
    ];

    for (input, want, want_err) in tests {
        let input = Name::new(input.to_owned())?;
        let result = input.pack(vec![], &mut Some(HashMap::new()), 0);
        if let Some(want_err) = want_err {
            if let Err(actual_err) = result {
                assert_eq!(want_err, actual_err);
            } else {
                assert!(false);
            }
            continue;
        } else {
            assert!(result.is_ok());
        }

        let buf = result.unwrap();

        let want = Name::new(want.to_owned())?;

        let mut got = Name::default();
        let n = got.unpack(&buf, 0)?;
        assert_eq!(
            n,
            buf.len(),
            "unpacked different amount than packed for {}: got = {}, want = {}",
            input,
            n,
            buf.len(),
        );

        assert_eq!(
            got, want,
            "unpacking packing of {}: got = {}, want = {}",
            input, got, want
        );
    }

    Ok(())
}

#[test]
fn test_incompressible_name() -> Result<(), Error> {
    let name = Name::new("example.com.".to_owned())?;
    let mut compression = Some(HashMap::new());
    let buf = name.pack(vec![], &mut compression, 0)?;
    let buf = name.pack(buf, &mut compression, 0)?;
    let mut n1 = Name::default();
    let off = n1.unpack_compressed(&buf, 0, false /* allowCompression */)?;
    let mut n2 = Name::default();
    let result = n2.unpack_compressed(&buf, off, false /* allowCompression */);
    if let Err(err) = result {
        assert_eq!(
            err,
            ERR_COMPRESSED_SRV.to_owned(),
            "unpacking compressed incompressible name with pointers: got {}, want = {}",
            err,
            ERR_COMPRESSED_SRV.to_owned()
        );
    } else {
        assert!(false);
    }

    Ok(())
}
