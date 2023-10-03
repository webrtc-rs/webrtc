use super::*;

#[test]
fn test_requested_address_family_string() -> Result<(), stun::Error> {
    assert_eq!(
        REQUESTED_FAMILY_IPV4.to_string(),
        "IPv4",
        "bad string {}, expected {}",
        REQUESTED_FAMILY_IPV4,
        "IPv4"
    );

    assert_eq!(
        REQUESTED_FAMILY_IPV6.to_string(),
        "IPv6",
        "bad string {}, expected {}",
        REQUESTED_FAMILY_IPV6,
        "IPv6"
    );

    assert_eq!(
        RequestedAddressFamily(0x04).to_string(),
        "unknown",
        "should be unknown"
    );

    Ok(())
}

#[test]
fn test_requested_address_family_add_to() -> Result<(), stun::Error> {
    let mut m = Message::new();
    let r = REQUESTED_FAMILY_IPV4;
    r.add_to(&mut m)?;
    m.write_header();

    //"GetFrom"
    {
        let mut decoded = Message::new();
        decoded.write(&m.raw)?;
        let mut req = RequestedAddressFamily::default();
        req.get_from(&decoded)?;
        assert_eq!(req, r, "Decoded {req}, expected {r}");

        //"HandleErr"
        {
            let mut m = Message::new();
            let mut handle = RequestedAddressFamily::default();
            if let Err(err) = handle.get_from(&m) {
                assert_eq!(
                    stun::Error::ErrAttributeNotFound,
                    err,
                    "{err} should be not found"
                );
            } else {
                panic!("expected error, but got ok");
            }
            m.add(ATTR_REQUESTED_ADDRESS_FAMILY, &[1, 2, 3]);
            if let Err(err) = handle.get_from(&m) {
                assert!(
                    is_attr_size_invalid(&err),
                    "IsAttrSizeInvalid should be true"
                );
            } else {
                panic!("expected error, but got ok");
            }
            m.reset();
            m.add(ATTR_REQUESTED_ADDRESS_FAMILY, &[5, 0, 0, 0]);
            assert!(
                handle.get_from(&m).is_err(),
                "should error on invalid value"
            );
        }
    }

    Ok(())
}
