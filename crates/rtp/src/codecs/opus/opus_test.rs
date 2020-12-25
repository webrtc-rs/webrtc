use super::*;

use std::io::BufReader;

use util::Error;

#[test]
fn test_opus_unmarshal() -> Result<(), Error> {
    let mut pck = OpusPacket::default();

    // Empty packet
    let empty_bytes = vec![];
    let mut reader = BufReader::new(empty_bytes.as_slice());
    let result = pck.depacketize(&mut reader);
    assert!(result.is_err(), "Result should be err in case of error");

    // Normal packet
    let raw_bytes = vec![0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x90];
    let mut reader = BufReader::new(raw_bytes.as_slice());
    pck.depacketize(&mut reader)?;
    assert_eq!(&raw_bytes, &pck.payload, "Payload must be same");

    Ok(())
}

#[test]
fn test_opus_payload() -> Result<(), Error> {
    let pck = OpusPayloader;
    let empty = vec![];
    let payload = vec![0x90, 0x90, 0x90];

    // Positive MTU, empty payload
    let mut reader = BufReader::new(empty.as_slice());
    let result = pck.payload(1, &mut reader)?;
    assert!(result.is_empty(), "Generated payload should be empty");

    // Positive MTU, small payload
    let mut reader = BufReader::new(payload.as_slice());
    let result = pck.payload(1, &mut reader)?;
    assert_eq!(result.len(), 1, "Generated payload should be the 1");

    // Negative MTU, small payload
    let mut reader = BufReader::new(payload.as_slice());
    let result = pck.payload(-1, &mut reader)?;
    assert_eq!(result.len(), 1, "Generated payload should be the 1");

    // Positive MTU, small payload
    let mut reader = BufReader::new(payload.as_slice());
    let result = pck.payload(2, &mut reader)?;
    assert_eq!(result.len(), 1, "Generated payload should be the 1");

    Ok(())
}
