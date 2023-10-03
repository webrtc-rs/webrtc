use super::*;
use crate::attributes::ATTR_SOFTWARE;
use crate::textattrs::TextAttribute;

#[test]
fn fingerprint_uses_crc_32_iso_hdlc() -> Result<()> {
    let mut m = Message::new();

    let a = TextAttribute {
        attr: ATTR_SOFTWARE,
        text: "software".to_owned(),
    };
    a.add_to(&mut m)?;
    m.write_header();

    FINGERPRINT.add_to(&mut m)?;
    m.write_header();

    assert_eq!(&m.raw[0..m.raw.len()-8], b"\x00\x00\x00\x14\x21\x12\xA4\x42\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x80\x22\x00\x08\x73\x6F\x66\x74\x77\x61\x72\x65");

    assert_eq!(m.raw[m.raw.len() - 4..], [0xe4, 0x4c, 0x33, 0xd9]);

    Ok(())
}

#[test]
fn test_fingerprint_check() -> Result<()> {
    let mut m = Message::new();
    let a = TextAttribute {
        attr: ATTR_SOFTWARE,
        text: "software".to_owned(),
    };
    a.add_to(&mut m)?;
    m.write_header();

    FINGERPRINT.add_to(&mut m)?;
    m.write_header();
    FINGERPRINT.check(&m)?;
    m.raw[3] += 1;

    let result = FINGERPRINT.check(&m);
    assert!(result.is_err(), "should error");

    Ok(())
}

#[test]
fn test_fingerprint_check_bad() -> Result<()> {
    let mut m = Message::new();
    let a = TextAttribute {
        attr: ATTR_SOFTWARE,
        text: "software".to_owned(),
    };
    a.add_to(&mut m)?;
    m.write_header();

    let result = FINGERPRINT.check(&m);
    assert!(result.is_err(), "should error");

    m.add(ATTR_FINGERPRINT, &[1, 2, 3]);

    let result = FINGERPRINT.check(&m);
    if let Err(err) = result {
        assert!(
            is_attr_size_invalid(&err),
            "IsAttrSizeInvalid should be true"
        );
    } else {
        panic!("Expected error, but got ok");
    }

    Ok(())
}
