use super::*;

#[test]
fn test_transport_cc_extension_too_small() -> Result<()> {
    let raw = Bytes::from_static(&[]);
    let result = TransportCcExtension::unmarshal(&raw);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_transport_cc_extension() -> Result<()> {
    let raw = Bytes::from_static(&[0x00, 0x02]);
    let t1 = TransportCcExtension::unmarshal(&raw)?;
    let t2 = TransportCcExtension {
        transport_sequence: 2,
    };
    assert_eq!(t1, t2);

    let mut dst = BytesMut::with_capacity(t2.marshal_size());
    t2.marshal_to(&mut dst)?;
    assert_eq!(raw, dst.freeze());

    Ok(())
}

#[test]
fn test_transport_cc_extension_extra_bytes() -> Result<()> {
    let raw = Bytes::from_static(&[0x00, 0x02, 0x00, 0xff, 0xff]);
    let t1 = TransportCcExtension::unmarshal(&raw)?;
    let t2 = TransportCcExtension {
        transport_sequence: 2,
    };
    assert_eq!(t1, t2);

    Ok(())
}
