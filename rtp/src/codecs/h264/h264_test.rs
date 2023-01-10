// Silence warning on `for i in 0..vec.len() { … }`:
#![allow(clippy::needless_range_loop)]

use super::*;

#[test]
fn test_h264_payload() -> Result<()> {
    let empty = Bytes::from_static(&[]);
    let small_payload = Bytes::from_static(&[0x90, 0x90, 0x90]);
    let multiple_payload = Bytes::from_static(&[0x00, 0x00, 0x01, 0x90, 0x00, 0x00, 0x01, 0x90]);
    let large_payload = Bytes::from_static(&[
        0x00, 0x00, 0x01, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x10, 0x11,
        0x12, 0x13, 0x14, 0x15,
    ]);
    let large_payload_packetized = vec![
        Bytes::from_static(&[0x1c, 0x80, 0x01, 0x02, 0x03]),
        Bytes::from_static(&[0x1c, 0x00, 0x04, 0x05, 0x06]),
        Bytes::from_static(&[0x1c, 0x00, 0x07, 0x08, 0x09]),
        Bytes::from_static(&[0x1c, 0x00, 0x10, 0x11, 0x12]),
        Bytes::from_static(&[0x1c, 0x40, 0x13, 0x14, 0x15]),
    ];

    let mut pck = H264Payloader::default();

    // Positive MTU, empty payload
    let result = pck.payload(1, &empty)?;
    assert!(result.is_empty(), "Generated payload should be empty");

    // 0 MTU, small payload
    let result = pck.payload(0, &small_payload)?;
    assert_eq!(result.len(), 0, "Generated payload should be empty");

    // Positive MTU, small payload
    let result = pck.payload(1, &small_payload)?;
    assert_eq!(result.len(), 0, "Generated payload should be empty");

    // Positive MTU, small payload
    let result = pck.payload(5, &small_payload)?;
    assert_eq!(result.len(), 1, "Generated payload should be the 1");
    assert_eq!(
        result[0].len(),
        small_payload.len(),
        "Generated payload should be the same size as original payload size"
    );

    // Multiple NALU in a single payload
    let result = pck.payload(5, &multiple_payload)?;
    assert_eq!(result.len(), 2, "2 nal units should be broken out");
    for i in 0..2 {
        assert_eq!(
            result[i].len(),
            1,
            "Payload {} of 2 is packed incorrectly",
            i + 1,
        );
    }

    // Large Payload split across multiple RTP Packets
    let result = pck.payload(5, &large_payload)?;
    assert_eq!(
        result, large_payload_packetized,
        "FU-A packetization failed"
    );

    // Nalu type 9 or 12
    let small_payload2 = Bytes::from_static(&[0x09, 0x00, 0x00]);
    let result = pck.payload(5, &small_payload2)?;
    assert_eq!(result.len(), 0, "Generated payload should be empty");

    Ok(())
}

#[test]
fn test_h264_packet_unmarshal() -> Result<()> {
    let single_payload = Bytes::from_static(&[0x90, 0x90, 0x90]);
    let single_payload_unmarshaled =
        Bytes::from_static(&[0x00, 0x00, 0x00, 0x01, 0x90, 0x90, 0x90]);
    let single_payload_unmarshaled_avc =
        Bytes::from_static(&[0x00, 0x00, 0x00, 0x03, 0x90, 0x90, 0x90]);

    let large_payload = Bytes::from_static(&[
        0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x10,
        0x11, 0x12, 0x13, 0x14, 0x15,
    ]);
    let large_payload_avc = Bytes::from_static(&[
        0x00, 0x00, 0x00, 0x10, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x10,
        0x11, 0x12, 0x13, 0x14, 0x15,
    ]);
    let large_payload_packetized = vec![
        Bytes::from_static(&[0x1c, 0x80, 0x01, 0x02, 0x03]),
        Bytes::from_static(&[0x1c, 0x00, 0x04, 0x05, 0x06]),
        Bytes::from_static(&[0x1c, 0x00, 0x07, 0x08, 0x09]),
        Bytes::from_static(&[0x1c, 0x00, 0x10, 0x11, 0x12]),
        Bytes::from_static(&[0x1c, 0x40, 0x13, 0x14, 0x15]),
    ];

    let single_payload_multi_nalu = Bytes::from_static(&[
        0x78, 0x00, 0x0f, 0x67, 0x42, 0xc0, 0x1f, 0x1a, 0x32, 0x35, 0x01, 0x40, 0x7a, 0x40, 0x3c,
        0x22, 0x11, 0xa8, 0x00, 0x05, 0x68, 0x1a, 0x34, 0xe3, 0xc8,
    ]);
    let single_payload_multi_nalu_unmarshaled = Bytes::from_static(&[
        0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0xc0, 0x1f, 0x1a, 0x32, 0x35, 0x01, 0x40, 0x7a, 0x40,
        0x3c, 0x22, 0x11, 0xa8, 0x00, 0x00, 0x00, 0x01, 0x68, 0x1a, 0x34, 0xe3, 0xc8,
    ]);
    let single_payload_multi_nalu_unmarshaled_avc = Bytes::from_static(&[
        0x00, 0x00, 0x00, 0x0f, 0x67, 0x42, 0xc0, 0x1f, 0x1a, 0x32, 0x35, 0x01, 0x40, 0x7a, 0x40,
        0x3c, 0x22, 0x11, 0xa8, 0x00, 0x00, 0x00, 0x05, 0x68, 0x1a, 0x34, 0xe3, 0xc8,
    ]);

    let incomplete_single_payload_multi_nalu = Bytes::from_static(&[
        0x78, 0x00, 0x0f, 0x67, 0x42, 0xc0, 0x1f, 0x1a, 0x32, 0x35, 0x01, 0x40, 0x7a, 0x40, 0x3c,
        0x22, 0x11,
    ]);

    let mut pkt = H264Packet::default();
    let mut avc_pkt = H264Packet {
        is_avc: true,
        ..Default::default()
    };

    let data = Bytes::from_static(&[]);
    let result = pkt.depacketize(&data);
    assert!(result.is_err(), "Unmarshal did not fail on nil payload");

    let data = Bytes::from_static(&[0x00, 0x00]);
    let result = pkt.depacketize(&data);
    assert!(
        result.is_err(),
        "Unmarshal accepted a packet that is too small for a payload and header"
    );

    let data = Bytes::from_static(&[0xFF, 0x00, 0x00]);
    let result = pkt.depacketize(&data);
    assert!(
        result.is_err(),
        "Unmarshal accepted a packet with a NALU Type we don't handle"
    );

    let result = pkt.depacketize(&incomplete_single_payload_multi_nalu);
    assert!(
        result.is_err(),
        "Unmarshal accepted a STAP-A packet with insufficient data"
    );

    let payload = pkt.depacketize(&single_payload)?;
    assert_eq!(
        payload, single_payload_unmarshaled,
        "Unmarshaling a single payload shouldn't modify the payload"
    );

    let payload = avc_pkt.depacketize(&single_payload)?;
    assert_eq!(
        payload, single_payload_unmarshaled_avc,
        "Unmarshaling a single payload into avc stream shouldn't modify the payload"
    );

    let mut large_payload_result = BytesMut::new();
    for p in &large_payload_packetized {
        let payload = pkt.depacketize(p)?;
        large_payload_result.put(&*payload.clone());
    }
    assert_eq!(
        large_payload_result.freeze(),
        large_payload,
        "Failed to unmarshal a large payload"
    );

    let mut large_payload_result_avc = BytesMut::new();
    for p in &large_payload_packetized {
        let payload = avc_pkt.depacketize(p)?;
        large_payload_result_avc.put(&*payload.clone());
    }
    assert_eq!(
        large_payload_result_avc.freeze(),
        large_payload_avc,
        "Failed to unmarshal a large payload into avc stream"
    );

    let payload = pkt.depacketize(&single_payload_multi_nalu)?;
    assert_eq!(
        payload, single_payload_multi_nalu_unmarshaled,
        "Failed to unmarshal a single packet with multiple NALUs"
    );

    let payload = avc_pkt.depacketize(&single_payload_multi_nalu)?;
    assert_eq!(
        payload, single_payload_multi_nalu_unmarshaled_avc,
        "Failed to unmarshal a single packet with multiple NALUs into avc stream"
    );

    Ok(())
}

#[test]
fn test_h264_partition_head_checker_is_partition_head() -> Result<()> {
    let h264 = H264Packet::default();
    let empty_nalu = Bytes::from_static(&[]);
    assert!(
        !h264.is_partition_head(&empty_nalu),
        "empty nalu must not be a partition head"
    );

    let single_nalu = Bytes::from_static(&[1, 0]);
    assert!(
        h264.is_partition_head(&single_nalu),
        "single nalu must be a partition head"
    );

    let stapa_nalu = Bytes::from_static(&[STAPA_NALU_TYPE, 0]);
    assert!(
        h264.is_partition_head(&stapa_nalu),
        "stapa nalu must be a partition head"
    );

    let fua_start_nalu = Bytes::from_static(&[FUA_NALU_TYPE, FU_START_BITMASK]);
    assert!(
        h264.is_partition_head(&fua_start_nalu),
        "fua start nalu must be a partition head"
    );

    let fua_end_nalu = Bytes::from_static(&[FUA_NALU_TYPE, FU_END_BITMASK]);
    assert!(
        !h264.is_partition_head(&fua_end_nalu),
        "fua end nalu must not be a partition head"
    );

    let fub_start_nalu = Bytes::from_static(&[FUB_NALU_TYPE, FU_START_BITMASK]);
    assert!(
        h264.is_partition_head(&fub_start_nalu),
        "fub start nalu must be a partition head"
    );

    let fub_end_nalu = Bytes::from_static(&[FUB_NALU_TYPE, FU_END_BITMASK]);
    assert!(
        !h264.is_partition_head(&fub_end_nalu),
        "fub end nalu must not be a partition head"
    );

    Ok(())
}

#[test]
fn test_h264_payloader_payload_sps_and_pps_handling() -> Result<()> {
    let mut pck = H264Payloader::default();
    let expected = vec![
        Bytes::from_static(&[
            0x78, 0x00, 0x03, 0x07, 0x00, 0x01, 0x00, 0x03, 0x08, 0x02, 0x03,
        ]),
        Bytes::from_static(&[0x05, 0x04, 0x05]),
    ];

    // When packetizing SPS and PPS are emitted with following NALU
    let res = pck.payload(1500, &Bytes::from_static(&[0x07, 0x00, 0x01]))?;
    assert!(res.is_empty(), "Generated payload should be empty");

    let res = pck.payload(1500, &Bytes::from_static(&[0x08, 0x02, 0x03]))?;
    assert!(res.is_empty(), "Generated payload should be empty");

    let actual = pck.payload(1500, &Bytes::from_static(&[0x05, 0x04, 0x05]))?;
    assert_eq!(actual, expected, "SPS and PPS aren't packed together");

    Ok(())
}
