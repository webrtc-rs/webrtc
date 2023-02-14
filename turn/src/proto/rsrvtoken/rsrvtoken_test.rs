use super::*;

#[test]
fn test_reservation_token() -> Result<(), stun::Error> {
    let mut m = Message::new();
    let mut v = vec![0; 8];
    v[2] = 33;
    v[7] = 1;
    let tk = ReservationToken(v);
    tk.add_to(&mut m)?;
    m.write_header();

    //"HandleErr"
    {
        let bad_tk = ReservationToken(vec![34, 45]);
        if let Err(err) = bad_tk.add_to(&mut m) {
            assert!(
                is_attr_size_invalid(&err),
                "IsAttrSizeInvalid should be true"
            );
        } else {
            panic!("expected error, but got ok");
        }
    }

    //"GetFrom"
    {
        let mut decoded = Message::new();
        decoded.write(&m.raw)?;
        let mut tok = ReservationToken::default();
        tok.get_from(&decoded)?;
        assert_eq!(tok, tk, "Decoded {tok:?}, expected {tk:?}");

        //"HandleErr"
        {
            let mut m = Message::new();
            let mut handle = ReservationToken::default();
            if let Err(err) = handle.get_from(&m) {
                assert_eq!(
                    stun::Error::ErrAttributeNotFound,
                    err,
                    "{err} should be not found"
                );
            } else {
                panic!("expected error, but got ok");
            }
            m.add(ATTR_RESERVATION_TOKEN, &[1, 2, 3]);
            if let Err(err) = handle.get_from(&m) {
                assert!(
                    is_attr_size_invalid(&err),
                    "IsAttrSizeInvalid should be true"
                );
            } else {
                panic!("expected error, got ok");
            }
        }
    }

    Ok(())
}
