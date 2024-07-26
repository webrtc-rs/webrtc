use bytes::BytesMut;

use crate::error::Result;

use super::*;

#[test]
fn test_playout_delay_extension_roundtrip() -> Result<()> {
    let test = PlayoutDelayExtension {
        max_delay: 2345,
        min_delay: 1234,
    };

    let mut raw = BytesMut::with_capacity(test.marshal_size());
    raw.resize(test.marshal_size(), 0);
    test.marshal_to(&mut raw)?;
    let raw = raw.freeze();
    let buf = &mut raw.clone();
    let out = PlayoutDelayExtension::unmarshal(buf)?;
    assert_eq!(test, out);

    Ok(())
}

#[test]
fn test_playout_delay_value_overflow() -> Result<()> {
    let test = PlayoutDelayExtension {
        max_delay: u16::MAX,
        min_delay: u16::MAX,
    };

    let mut dst = BytesMut::with_capacity(test.marshal_size());
    dst.resize(test.marshal_size(), 0);
    let result = test.marshal_to(&mut dst);
    assert!(result.is_err());

    Ok(())
}
