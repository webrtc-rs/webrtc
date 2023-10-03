use bytes::Bytes;

use super::*;

#[test]
fn test_receiver_estimated_maximum_bitrate_marshal() {
    let input = ReceiverEstimatedMaximumBitrate {
        sender_ssrc: 1,
        bitrate: 8927168.0,
        ssrcs: vec![1215622422],
    };

    let expected = Bytes::from_static(&[
        143, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 82, 69, 77, 66, 1, 26, 32, 223, 72, 116, 237, 22,
    ]);

    let output = input.marshal().unwrap();
    assert_eq!(output, expected);
}

#[test]
fn test_receiver_estimated_maximum_bitrate_unmarshal() {
    // Real data sent by Chrome while watching a 6Mb/s stream
    let mut input = Bytes::from_static(&[
        143, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 82, 69, 77, 66, 1, 26, 32, 223, 72, 116, 237, 22,
    ]);

    // mantissa = []byte{26 & 3, 32, 223} = []byte{2, 32, 223} = 139487
    // exp = 26 >> 2 = 6
    // bitrate = 139487 * 2^6 = 139487 * 64 = 8927168 = 8.9 Mb/s
    let expected = ReceiverEstimatedMaximumBitrate {
        sender_ssrc: 1,
        bitrate: 8927168.0,
        ssrcs: vec![1215622422],
    };

    let packet = ReceiverEstimatedMaximumBitrate::unmarshal(&mut input).unwrap();
    assert_eq!(packet, expected);
}

#[test]
fn test_receiver_estimated_maximum_bitrate_truncate() {
    let input = Bytes::from_static(&[
        143, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 82, 69, 77, 66, 1, 26, 32, 223, 72, 116, 237, 22,
    ]);

    // Make sure that we're interpreting the bitrate correctly.
    // For the above example, we have:

    // mantissa = 139487
    // exp = 6
    // bitrate = 8927168

    let mut buf = input.clone();
    let mut packet = ReceiverEstimatedMaximumBitrate::unmarshal(&mut buf).unwrap();
    assert_eq!(packet.bitrate, 8927168.0);

    // Just verify marshal produces the same input.
    let output = packet.marshal().unwrap();
    assert_eq!(output, input);

    // If we subtract the bitrate by 1, we'll round down a lower mantissa
    packet.bitrate -= 1.0;

    // bitrate = 8927167
    // mantissa = 139486
    // exp = 6

    let mut output = packet.marshal().unwrap();
    assert_ne!(output, input);
    let expected = Bytes::from_static(&[
        143, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 82, 69, 77, 66, 1, 26, 32, 222, 72, 116, 237, 22,
    ]);
    assert_eq!(output, expected);

    // Which if we actually unmarshal again, we'll find that it's actually decreased by 63 (which is exp)
    // mantissa = 139486
    // exp = 6
    // bitrate = 8927104

    let packet = ReceiverEstimatedMaximumBitrate::unmarshal(&mut output).unwrap();
    assert_eq!(8927104.0, packet.bitrate);
}

#[test]
fn test_receiver_estimated_maximum_bitrate_overflow() {
    // Marshal a packet with the maximum possible bitrate.
    let packet = ReceiverEstimatedMaximumBitrate {
        bitrate: f32::MAX,
        ..Default::default()
    };

    // mantissa = 262143 = 0x3FFFF
    // exp = 63

    let expected = Bytes::from_static(&[
        143, 206, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 82, 69, 77, 66, 0, 255, 255, 255,
    ]);

    let output = packet.marshal().unwrap();
    assert_eq!(output, expected);

    // mantissa = 262143
    // exp = 63
    // bitrate = 0xFFFFC00000000000

    let mut buf = output;
    let packet = ReceiverEstimatedMaximumBitrate::unmarshal(&mut buf).unwrap();
    assert_eq!(packet.bitrate, f32::from_bits(0x67FFFFC0));

    // Make sure we marshal to the same result again.
    let output = packet.marshal().unwrap();
    assert_eq!(output, expected);

    // Finally, try unmarshalling one number higher than we used to be able to handle.
    let mut input = Bytes::from_static(&[
        143, 206, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 82, 69, 77, 66, 0, 188, 0, 0,
    ]);
    let packet = ReceiverEstimatedMaximumBitrate::unmarshal(&mut input).unwrap();
    assert_eq!(packet.bitrate, f32::from_bits(0x62800000));
}
