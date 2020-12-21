use super::*;
use crate::textattrs::TextAttribute;

use crate::attributes::ATTR_SOFTWARE;
use util::Error;

#[test]
fn test_fingerprint_check() -> Result<(), Error> {
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
fn test_fingerprint_check_bad() -> Result<(), Error> {
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
        assert!(false, "Expected error, but got ok");
    }

    Ok(())
}
