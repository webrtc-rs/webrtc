use super::*;

#[test]
fn test_vp8_unmarshal() -> Result<()> {
    let mut pck = Vp8Packet::default();

    // Empty packet
    let empty_bytes = Bytes::from_static(&[]);
    let result = pck.depacketize(&empty_bytes);
    assert!(result.is_err(), "Result should be err in case of error");

    // Payload smaller than header size
    let small_bytes = Bytes::from_static(&[0x00, 0x11, 0x22]);
    let result = pck.depacketize(&small_bytes);
    assert!(result.is_err(), "Result should be err in case of error");

    // Payload smaller than header size
    let small_bytes = Bytes::from_static(&[0x00, 0x11]);
    let result = pck.depacketize(&small_bytes);
    assert!(result.is_err(), "Result should be err in case of error");

    // Normal packet
    let raw_bytes = Bytes::from_static(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x90]);
    pck.depacketize(&raw_bytes).expect("Normal packet");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");

    // Header size, only X
    let raw_bytes = Bytes::from_static(&[0x80, 0x00, 0x00, 0x00]);
    pck.depacketize(&raw_bytes).expect("Only X");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and I, PID 16bits
    let raw_bytes = Bytes::from_static(&[0x80, 0x80, 0x81, 0x00, 0x00]);
    pck.depacketize(&raw_bytes).expect("X and I, PID 16bits");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 1, "I must be 1");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and L
    let raw_bytes = Bytes::from_static(&[0x80, 0x40, 0x00, 0x00]);
    pck.depacketize(&raw_bytes).expect("X and L");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 1, "L must be 1");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and T
    let raw_bytes = Bytes::from_static(&[0x80, 0x20, 0x00, 0x00]);
    pck.depacketize(&raw_bytes).expect("X and T");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 1, "T must be 1");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and K
    let raw_bytes = Bytes::from_static(&[0x80, 0x10, 0x00, 0x00]);
    pck.depacketize(&raw_bytes).expect("X and K");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 1, "K must be 1");

    // Header size, all flags and 8bit picture_id
    let raw_bytes = Bytes::from_static(&[0xff, 0xff, 0x00, 0x00, 0x00, 0x00]);
    pck.depacketize(&raw_bytes)
        .expect("all flags and 8bit picture_id");
    assert!(!pck.payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 1, "I must be 1");
    assert_eq!(pck.l, 1, "L must be 1");
    assert_eq!(pck.t, 1, "T must be 1");
    assert_eq!(pck.k, 1, "K must be 1");

    // Header size, all flags and 16bit picture_id
    let raw_bytes = Bytes::from_static(&[0xff, 0xff, 0x80, 0x00, 0x00, 0x00, 0x00]);
    pck.depacketize(&raw_bytes)
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
fn test_vp8_payload() -> Result<()> {
    let mut pck = Vp8Payloader::default();
    let empty = Bytes::from_static(&[]);
    let payload = Bytes::from_static(&[0x90, 0x90, 0x90]);

    // Positive MTU, empty payload
    let result = pck.payload(1, &empty)?;
    assert!(result.is_empty(), "Generated payload should be empty");

    // Positive MTU, small payload
    let result = pck.payload(1, &payload)?;
    assert_eq!(result.len(), 0, "Generated payload should be empty");

    // Positive MTU, small payload
    let result = pck.payload(2, &payload)?;
    assert_eq!(
        result.len(),
        payload.len(),
        "Generated payload should be the same size as original payload size"
    );

    Ok(())
}
