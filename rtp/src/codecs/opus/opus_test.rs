use super::*;

#[test]
fn test_opus_unmarshal() -> Result<()> {
    let mut pck = OpusPacket;

    // Empty packet
    let empty_bytes = Bytes::from_static(&[]);
    let result = pck.depacketize(&empty_bytes);
    assert!(result.is_err(), "Result should be err in case of error");

    // Normal packet
    let raw_bytes = Bytes::from_static(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x90]);
    let payload = pck.depacketize(&raw_bytes)?;
    assert_eq!(&raw_bytes, &payload, "Payload must be same");

    Ok(())
}

#[test]
fn test_opus_payload() -> Result<()> {
    let mut pck = OpusPayloader;
    let empty = Bytes::from_static(&[]);
    let payload = Bytes::from_static(&[0x90, 0x90, 0x90]);

    // Positive MTU, empty payload
    let result = pck.payload(1, &empty)?;
    assert!(result.is_empty(), "Generated payload should be empty");

    // Positive MTU, small payload
    let result = pck.payload(1, &payload)?;
    assert_eq!(result.len(), 1, "Generated payload should be the 1");

    // Positive MTU, small payload
    let result = pck.payload(2, &payload)?;
    assert_eq!(result.len(), 1, "Generated payload should be the 1");

    Ok(())
}

#[test]
fn test_opus_is_partition_head() -> Result<()> {
    let opus = OpusPacket;
    //"NormalPacket"
    assert!(
        opus.is_partition_head(&Bytes::from_static(&[0x00, 0x00])),
        "All OPUS RTP packet should be the head of a new partition"
    );

    Ok(())
}
