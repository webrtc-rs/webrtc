use super::*;

use std::io::{BufReader, BufWriter};

#[test]
fn test_transport_cc_extension_too_small() -> Result<(), Error> {
    let raw: Vec<u8> = vec![];
    let mut reader = BufReader::new(raw.as_slice());
    let result = TransportCCExtension::unmarshal(&mut reader);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_transport_cc_extension() -> Result<(), Error> {
    let raw: Vec<u8> = vec![0x00, 0x02];
    let mut reader = BufReader::new(raw.as_slice());
    let t1 = TransportCCExtension::unmarshal(&mut reader)?;
    let t2 = TransportCCExtension {
        transport_sequence: 2,
    };
    assert_eq!(t1, t2);

    let mut dst: Vec<u8> = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(dst.as_mut());
        t2.marshal(&mut writer)?;
    }
    assert_eq!(raw, dst);

    Ok(())
}

#[test]
fn test_transport_cc_extension_extra_bytes() -> Result<(), Error> {
    let raw: Vec<u8> = vec![0x00, 0x02, 0x00, 0xff, 0xff];
    let mut reader = BufReader::new(raw.as_slice());
    let t1 = TransportCCExtension::unmarshal(&mut reader)?;
    let t2 = TransportCCExtension {
        transport_sequence: 2,
    };
    assert_eq!(t1, t2);

    Ok(())
}
