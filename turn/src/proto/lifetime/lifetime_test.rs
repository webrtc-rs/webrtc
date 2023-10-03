use super::*;

#[test]
fn test_lifetime_string() -> Result<(), stun::Error> {
    let l = Lifetime(Duration::from_secs(10));
    assert_eq!(l.to_string(), "10s", "bad string {l}, expected 10s");

    Ok(())
}

#[test]
fn test_lifetime_add_to() -> Result<(), stun::Error> {
    let mut m = Message::new();
    let l = Lifetime(Duration::from_secs(10));
    l.add_to(&mut m)?;
    m.write_header();

    //"GetFrom"
    {
        let mut decoded = Message::new();
        decoded.write(&m.raw)?;

        let mut life = Lifetime::default();
        life.get_from(&decoded)?;
        assert_eq!(life, l, "Decoded {life}, expected {l}");

        //"HandleErr"
        {
            let mut m = Message::new();
            let mut n_handle = Lifetime::default();
            if let Err(err) = n_handle.get_from(&m) {
                assert_eq!(
                    stun::Error::ErrAttributeNotFound,
                    err,
                    "{err} should be not found"
                );
            } else {
                panic!("expected error, but got ok");
            }
            m.add(ATTR_LIFETIME, &[1, 2, 3]);

            if let Err(err) = n_handle.get_from(&m) {
                assert!(
                    is_attr_size_invalid(&err),
                    "IsAttrSizeInvalid should be true"
                );
            } else {
                panic!("expected error, but got ok");
            }
        }
    }

    Ok(())
}
