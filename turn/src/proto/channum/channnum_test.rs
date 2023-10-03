use super::*;

#[test]
fn test_channel_number_string() -> Result<(), stun::Error> {
    let n = ChannelNumber(112);
    assert_eq!(n.to_string(), "112", "bad string {n}, expected 112");
    Ok(())
}

/*
#[test]
fn test_channel_number_NoAlloc() -> Result<(), stun::Error> {
    let mut m = Message::default();

        if wasAllocs(func() {
            // Case with ChannelNumber on stack.
            n: = ChannelNumber(6)
            n.AddTo(m) //nolint
            m.Reset()
        }) {
        t.Error("Unexpected allocations")
    }

        n: = ChannelNumber(12)
        nP: = &n
        if wasAllocs(func() {
            // On heap.
            nP.AddTo(m) //nolint
            m.Reset()
        }) {
        t.Error("Unexpected allocations")
    }
    Ok(())
}
*/

#[test]
fn test_channel_number_add_to() -> Result<(), stun::Error> {
    let mut m = Message::new();
    let n = ChannelNumber(6);
    n.add_to(&mut m)?;
    m.write_header();

    //"GetFrom"
    {
        let mut decoded = Message::new();
        decoded.write(&m.raw)?;

        let mut num_decoded = ChannelNumber::default();
        num_decoded.get_from(&decoded)?;
        assert_eq!(num_decoded, n, "Decoded {num_decoded}, expected {n}");

        //"HandleErr"
        {
            let mut m = Message::new();
            let mut n_handle = ChannelNumber::default();
            if let Err(err) = n_handle.get_from(&m) {
                assert_eq!(
                    stun::Error::ErrAttributeNotFound,
                    err,
                    "{err} should be not found"
                );
            } else {
                panic!("expected error, but got ok");
            }

            m.add(ATTR_CHANNEL_NUMBER, &[1, 2, 3]);

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
