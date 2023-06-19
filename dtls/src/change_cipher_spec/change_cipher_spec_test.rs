use std::io::{BufReader, BufWriter};

use super::*;

#[test]
fn test_change_cipher_spec_round_trip() -> Result<()> {
    let c = ChangeCipherSpec {};
    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        c.marshal(&mut writer)?;
    }

    let mut reader = BufReader::new(raw.as_slice());
    let cnew = ChangeCipherSpec::unmarshal(&mut reader)?;
    assert_eq!(
        c, cnew,
        "ChangeCipherSpec round trip: got {cnew:?}, want {c:?}"
    );

    Ok(())
}

#[test]
fn test_change_cipher_spec_invalid() -> Result<()> {
    let data = vec![0x00];

    let mut reader = BufReader::new(data.as_slice());
    let result = ChangeCipherSpec::unmarshal(&mut reader);

    match result {
        Ok(_) => panic!("must be error"),
        Err(err) => assert_eq!(err.to_string(), Error::ErrInvalidCipherSpec.to_string()),
    };

    Ok(())
}
