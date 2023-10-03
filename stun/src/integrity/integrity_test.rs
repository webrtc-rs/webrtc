use super::*;
use crate::agent::TransactionId;
use crate::attributes::ATTR_SOFTWARE;
use crate::fingerprint::FINGERPRINT;
use crate::textattrs::TextAttribute;

#[test]
fn test_message_integrity_add_to_simple() -> Result<()> {
    let i = MessageIntegrity::new_long_term_integrity(
        "user".to_owned(),
        "realm".to_owned(),
        "pass".to_owned(),
    );
    let expected = vec![
        0x84, 0x93, 0xfb, 0xc5, 0x3b, 0xa5, 0x82, 0xfb, 0x4c, 0x04, 0x4c, 0x45, 0x6b, 0xdc, 0x40,
        0xeb,
    ];
    assert_eq!(i.0, expected, "{}", Error::ErrIntegrityMismatch);

    //"Check"
    {
        let mut m = Message::new();
        m.write_header();
        i.add_to(&mut m)?;
        let a = TextAttribute {
            attr: ATTR_SOFTWARE,
            text: "software".to_owned(),
        };
        a.add_to(&mut m)?;
        m.write_header();

        let mut d_m = Message::new();
        d_m.raw = m.raw.clone();
        d_m.decode()?;
        i.check(&mut d_m)?;

        d_m.raw[24] += 12; // HMAC now invalid
        d_m.decode()?;
        let result = i.check(&mut d_m);
        assert!(result.is_err(), "should be invalid");
    }

    Ok(())
}

#[test]
fn test_message_integrity_with_fingerprint() -> Result<()> {
    let mut m = Message::new();
    m.transaction_id = TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 0]);
    m.write_header();
    let a = TextAttribute {
        attr: ATTR_SOFTWARE,
        text: "software".to_owned(),
    };
    a.add_to(&mut m)?;

    let i = MessageIntegrity::new_short_term_integrity("pwd".to_owned());
    assert_eq!(i.to_string(), "KEY: 0x[70, 77, 64]", "bad string {i}");
    let result = i.check(&mut m);
    assert!(result.is_err(), "should error");

    i.add_to(&mut m)?;
    FINGERPRINT.add_to(&mut m)?;
    i.check(&mut m)?;
    m.raw[24] = 33;
    m.decode()?;
    let result = i.check(&mut m);
    assert!(result.is_err(), "mismatch expected");

    Ok(())
}

#[test]
fn test_message_integrity() -> Result<()> {
    let mut m = Message::new();
    let i = MessageIntegrity::new_short_term_integrity("password".to_owned());
    m.write_header();
    i.add_to(&mut m)?;
    m.get(ATTR_MESSAGE_INTEGRITY)?;
    Ok(())
}

#[test]
fn test_message_integrity_before_fingerprint() -> Result<()> {
    let mut m = Message::new();
    m.write_header();
    FINGERPRINT.add_to(&mut m)?;
    let i = MessageIntegrity::new_short_term_integrity("password".to_owned());
    let result = i.add_to(&mut m);
    assert!(result.is_err(), "should error");

    Ok(())
}
