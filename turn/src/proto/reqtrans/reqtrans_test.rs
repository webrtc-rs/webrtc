use super::*;

#[test]
fn test_requested_transport_string() -> Result<(), stun::Error> {
    let mut r = RequestedTransport {
        protocol: PROTO_UDP,
    };
    assert_eq!(
        r.to_string(),
        "protocol: UDP",
        "bad string {}, expected {}",
        r,
        "protocol: UDP",
    );
    r.protocol = Protocol(254);
    if r.to_string() != "protocol: 254" {
        assert_eq!(
            r.to_string(),
            "protocol: UDP",
            "bad string {}, expected {}",
            r,
            "protocol: 254",
        );
    }

    Ok(())
}

#[test]
fn test_requested_transport_add_to() -> Result<(), stun::Error> {
    let mut m = Message::new();
    let r = RequestedTransport {
        protocol: PROTO_UDP,
    };
    r.add_to(&mut m)?;
    m.write_header();

    //"GetFrom"
    {
        let mut decoded = Message::new();
        decoded.write(&m.raw)?;
        let mut req = RequestedTransport {
            protocol: PROTO_UDP,
        };
        req.get_from(&decoded)?;
        assert_eq!(req, r, "Decoded {req}, expected {r}");

        //"HandleErr"
        {
            let mut m = Message::new();
            let mut handle = RequestedTransport::default();
            if let Err(err) = handle.get_from(&m) {
                assert_eq!(
                    stun::Error::ErrAttributeNotFound,
                    err,
                    "{err} should be not found"
                );
            } else {
                panic!("expected error, got ok");
            }

            m.add(ATTR_REQUESTED_TRANSPORT, &[1, 2, 3]);
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
