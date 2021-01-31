use super::*;

use std::io::BufReader;

#[test]
fn test_vp8_unmarshal() -> Result<(), Error> {
    let mut pck = VP8Packet::default();

    // Empty packet
    let empty_bytes = vec![];
    let mut reader = BufReader::new(empty_bytes.as_slice());
    let result = pck.depacketize(&mut reader);
    assert!(result.is_err(), "Result should be err in case of error");

    // Payload smaller than header size
    let small_bytes = vec![0x00, 0x11, 0x22];
    let mut reader = BufReader::new(small_bytes.as_slice());
    let result = pck.depacketize(&mut reader);
    assert!(result.is_err(), "Result should be err in case of error");

    // Payload smaller than header size
    let small_bytes = vec![0x00, 0x11];
    let mut reader = BufReader::new(small_bytes.as_slice());
    let result = pck.depacketize(&mut reader);
    assert!(result.is_err(), "Result should be err in case of error");

    // Normal packet
    let raw_bytes = vec![0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x90];
    let mut reader = BufReader::new(raw_bytes.as_slice());
    pck.depacketize(&mut reader).expect("Normal packet");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");

    // Header size, only X
    let raw_bytes = vec![0x80, 0x00, 0x00, 0x00];
    let mut reader = BufReader::new(raw_bytes.as_slice());
    pck.depacketize(&mut reader).expect("Only X");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and I, PID 16bits
    let raw_bytes = vec![0x80, 0x80, 0x81, 0x00, 0x00];
    let mut reader = BufReader::new(raw_bytes.as_slice());
    pck.depacketize(&mut reader).expect("X and I, PID 16bits");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 1, "I must be 1");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and L
    let raw_bytes = vec![0x80, 0x40, 0x00, 0x00];
    let mut reader = BufReader::new(raw_bytes.as_slice());
    pck.depacketize(&mut reader).expect("X and L");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 1, "L must be 1");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and T
    let raw_bytes = vec![0x80, 0x20, 0x00, 0x00];
    let mut reader = BufReader::new(raw_bytes.as_slice());
    pck.depacketize(&mut reader).expect("X and T");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 1, "T must be 1");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and K
    let raw_bytes = vec![0x80, 0x10, 0x00, 0x00];
    let mut reader = BufReader::new(raw_bytes.as_slice());
    pck.depacketize(&mut reader).expect("X and K");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 1, "K must be 1");

    // Header size, all flags and 8bit picture_id
    let raw_bytes = vec![0xff, 0xff, 0x00, 0x00, 0x00, 0x00];
    let mut reader = BufReader::new(raw_bytes.as_slice());
    pck.depacketize(&mut reader)
        .expect("all flags and 8bit picture_id");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 1, "I must be 1");
    assert_eq!(pck.l, 1, "L must be 1");
    assert_eq!(pck.t, 1, "T must be 1");
    assert_eq!(pck.k, 1, "K must be 1");

    // Header size, all flags and 16bit picture_id
    let raw_bytes = vec![0xff, 0xff, 0x80, 0x00, 0x00, 0x00, 0x00];
    let mut reader = BufReader::new(raw_bytes.as_slice());
    pck.depacketize(&mut reader)
        .expect("all flags and 16bit picture_id");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 1, "I must be 1");
    assert_eq!(pck.l, 1, "L must be 1");
    assert_eq!(pck.t, 1, "T must be 1");
    assert_eq!(pck.k, 1, "K must be 1");

    Ok(())
}

#[test]
fn test_vp8_payload() -> Result<(), Error> {
    let pck = VP8Payloader;
    let empty = vec![];
    let payload = vec![0x90, 0x90, 0x90];

    // Positive MTU, empty payload
    let mut reader = BufReader::new(empty.as_slice());
    let result = pck.payload(1, &mut reader)?;
    assert!(result.is_empty(), "Generated payload should be empty");

    // Positive MTU, small payload
    let mut reader = BufReader::new(payload.as_slice());
    let result = pck.payload(1, &mut reader)?;
    assert_eq!(result.len(), 0, "Generated payload should be empty");

    // Negative MTU, small payload
    let mut reader = BufReader::new(payload.as_slice());
    let result = pck.payload(-1, &mut reader)?;
    assert_eq!(result.len(), 0, "Generated payload should be empty");

    // Positive MTU, small payload
    let mut reader = BufReader::new(payload.as_slice());
    let result = pck.payload(2, &mut reader)?;
    assert_eq!(
        result.len(),
        payload.len(),
        "Generated payload should be the same size as original payload size"
    );

    Ok(())
}
