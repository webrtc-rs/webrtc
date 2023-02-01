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
    let payload = pck.depacketize(&raw_bytes).expect("Normal packet");
    assert!(!payload.is_empty(), "Payload must be not empty");

    // Header size, only X
    let raw_bytes = Bytes::from_static(&[0x80, 0x00, 0x00, 0x00]);
    let payload = pck.depacketize(&raw_bytes).expect("Only X");
    assert!(!payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and I, PID 16bits
    let raw_bytes = Bytes::from_static(&[0x80, 0x80, 0x81, 0x00, 0x00]);
    let payload = pck.depacketize(&raw_bytes).expect("X and I, PID 16bits");
    assert!(!payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 1, "I must be 1");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and L
    let raw_bytes = Bytes::from_static(&[0x80, 0x40, 0x00, 0x00]);
    let payload = pck.depacketize(&raw_bytes).expect("X and L");
    assert!(!payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 1, "L must be 1");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and T
    let raw_bytes = Bytes::from_static(&[0x80, 0x20, 0x00, 0x00]);
    let payload = pck.depacketize(&raw_bytes).expect("X and T");
    assert!(!payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 1, "T must be 1");
    assert_eq!(pck.k, 0, "K must be 0");

    // Header size, X and K
    let raw_bytes = Bytes::from_static(&[0x80, 0x10, 0x00, 0x00]);
    let payload = pck.depacketize(&raw_bytes).expect("X and K");
    assert!(!payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 0, "I must be 0");
    assert_eq!(pck.l, 0, "L must be 0");
    assert_eq!(pck.t, 0, "T must be 0");
    assert_eq!(pck.k, 1, "K must be 1");

    // Header size, all flags and 8bit picture_id
    let raw_bytes = Bytes::from_static(&[0xff, 0xff, 0x00, 0x00, 0x00, 0x00]);
    let payload = pck
        .depacketize(&raw_bytes)
        .expect("all flags and 8bit picture_id");
    assert!(!payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 1, "I must be 1");
    assert_eq!(pck.l, 1, "L must be 1");
    assert_eq!(pck.t, 1, "T must be 1");
    assert_eq!(pck.k, 1, "K must be 1");

    // Header size, all flags and 16bit picture_id
    let raw_bytes = Bytes::from_static(&[0xff, 0xff, 0x80, 0x00, 0x00, 0x00, 0x00]);
    let payload = pck
        .depacketize(&raw_bytes)
        .expect("all flags and 16bit picture_id");
    assert!(!payload.is_empty(), "Payload must be not empty");
    assert_eq!(pck.x, 1, "X must be 1");
    assert_eq!(pck.i, 1, "I must be 1");
    assert_eq!(pck.l, 1, "L must be 1");
    assert_eq!(pck.t, 1, "T must be 1");
    assert_eq!(pck.k, 1, "K must be 1");

    Ok(())
}

#[test]
fn test_vp8_payload() -> Result<()> {
    let tests = vec![
        (
            "WithoutPictureID",
            Vp8Payloader::default(),
            2,
            vec![
                Bytes::from_static(&[0x90, 0x90, 0x90]),
                Bytes::from_static(&[0x91, 0x91]),
            ],
            vec![
                vec![
                    Bytes::from_static(&[0x10, 0x90]),
                    Bytes::from_static(&[0x00, 0x90]),
                    Bytes::from_static(&[0x00, 0x90]),
                ],
                vec![
                    Bytes::from_static(&[0x10, 0x91]),
                    Bytes::from_static(&[0x00, 0x91]),
                ],
            ],
        ),
        (
            "WithPictureID_1byte",
            Vp8Payloader {
                enable_picture_id: true,
                picture_id: 0x20,
            },
            5,
            vec![
                Bytes::from_static(&[0x90, 0x90, 0x90]),
                Bytes::from_static(&[0x91, 0x91]),
            ],
            vec![
                vec![
                    Bytes::from_static(&[0x90, 0x80, 0x20, 0x90, 0x90]),
                    Bytes::from_static(&[0x80, 0x80, 0x20, 0x90]),
                ],
                vec![Bytes::from_static(&[0x90, 0x80, 0x21, 0x91, 0x91])],
            ],
        ),
        (
            "WithPictureID_2bytes",
            Vp8Payloader {
                enable_picture_id: true,
                picture_id: 0x120,
            },
            6,
            vec![
                Bytes::from_static(&[0x90, 0x90, 0x90]),
                Bytes::from_static(&[0x91, 0x91]),
            ],
            vec![
                vec![
                    Bytes::from_static(&[0x90, 0x80, 0x81, 0x20, 0x90, 0x90]),
                    Bytes::from_static(&[0x80, 0x80, 0x81, 0x20, 0x90]),
                ],
                vec![Bytes::from_static(&[0x90, 0x80, 0x81, 0x21, 0x91, 0x91])],
            ],
        ),
    ];

    for (name, mut pck, mtu, payloads, expected) in tests {
        for (i, payload) in payloads.iter().enumerate() {
            let actual = pck.payload(mtu, payload)?;
            assert_eq!(expected[i], actual, "{name}: Generated packet[{i}] differs");
        }
    }

    Ok(())
}

#[test]
fn test_vp8_payload_eror() -> Result<()> {
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

#[test]
fn test_vp8_partition_head_checker_is_partition_head() -> Result<()> {
    let vp8 = Vp8Packet::default();

    //"SmallPacket"
    assert!(
        !vp8.is_partition_head(&Bytes::from_static(&[0x00])),
        "Small packet should not be the head of a new partition"
    );

    //"SFlagON",
    assert!(
        vp8.is_partition_head(&Bytes::from_static(&[0x10, 0x00, 0x00, 0x00])),
        "Packet with S flag should be the head of a new partition"
    );

    //"SFlagOFF"
    assert!(
        !vp8.is_partition_head(&Bytes::from_static(&[0x00, 0x00, 0x00, 0x00])),
        "Packet without S flag should not be the head of a new partition"
    );

    Ok(())
}
