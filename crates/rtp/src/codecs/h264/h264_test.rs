#[cfg(test)]
mod tests {
    use crate::{
        codecs::h264::h264_def::*,
        errors::RTPError,
        packetizer::{Depacketizer, Payloader},
    };

    #[test]
    fn test_h264_payload() {
        let small_payload = vec![0x90u8, 0x90, 0x90];
        let multiple_payload = vec![0x00, 0x00, 0x01, 0x90, 0x00, 0x00, 0x01, 0x90];
        let large_payload = vec![
            0x00, 0x00, 0x01, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x10,
            0x11, 0x12, 0x13, 0x14, 0x15,
        ];
        let large_payload_packetized = vec![
            vec![0x1c, 0x80, 0x01, 0x02, 0x03],
            vec![0x1c, 0x00, 0x04, 0x05, 0x06],
            vec![0x1c, 0x00, 0x07, 0x08, 0x09],
            vec![0x1c, 0x00, 0x10, 0x11, 0x12],
            vec![0x1c, 0x40, 0x13, 0x14, 0x15],
        ];

        let pck = H264Payloader;

        // Positive MTU, nil payload
        let res = pck.payload(1, &[]);
        assert!(res.is_empty(), "Generated payload should be empty");

        // Negative MTU, small payload
        let res = pck.payload(0, small_payload[..].into());
        assert!(res.is_empty(), "Generated payload should be empty");

        // 0 MTU, small payload
        let res = pck.payload(0, small_payload[..].into());
        assert!(res.is_empty(), "Generated payload should be empty");

        // Positive MTU, small payload
        let res = pck.payload(1, small_payload[..].into());
        assert!(res.is_empty(), "Generated payload should be empty");

        // Positive MTU, small payload
        let res = pck.payload(5, small_payload[..].into());
        assert!(!res.is_empty(), "Generated payload should not be empty");
        assert_eq!(
            res[0].len(),
            small_payload.len(),
            "Generated payload should be the same size as original payload size"
        );

        // Multiple NALU in a single payload
        let res = pck.payload(5, multiple_payload[..].into());
        assert_eq!(res.len(), 2, "2 nal units should be broken out");
        for i in 0..2 {
            assert_eq!(
                res[i].len(),
                1,
                "Payload {} of 2 is packed incorrectly",
                i + 1,
            );
        }

        // Large Payload split across multiple RTP Packets
        let res = pck.payload(5, large_payload[..].into());
        assert_eq!(res, large_payload_packetized, "FU-A packetization failed");

        // Nalu type 9 or 12
        let result = pck.payload(5, [0x09u8, 0x00, 0x00][..].into());
        assert!(result.is_empty(), "Generated payload should be empty");
    }

    #[test]
    fn test_h264packet_unmarshal() -> Result<(), RTPError> {
        let mut single_payload: Vec<u8> = vec![0x90, 0x90, 0x90];
        let single_payload_unmarshaled: Vec<u8> = vec![0x00, 0x00, 0x00, 0x01, 0x90, 0x90, 0x90];

        let large_payload: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09,
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
        ];
        let large_payload_packetized: Vec<Vec<u8>> = vec![
            vec![0x1c, 0x80, 0x01, 0x02, 0x03],
            vec![0x1c, 0x00, 0x04, 0x05, 0x06],
            vec![0x1c, 0x00, 0x07, 0x08, 0x09],
            vec![0x1c, 0x00, 0x10, 0x11, 0x12],
            vec![0x1c, 0x40, 0x13, 0x14, 0x15],
        ];

        let mut single_payload_multi_nalu: Vec<u8> = vec![
            0x78, 0x00, 0x0f, 0x67, 0x42, 0xc0, 0x1f, 0x1a, 0x32, 0x35, 0x01, 0x40, 0x7a, 0x40,
            0x3c, 0x22, 0x11, 0xa8, 0x00, 0x05, 0x68, 0x1a, 0x34, 0xe3, 0xc8,
        ];
        let single_payload_multi_nalu_unmarshaled: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0xc0, 0x1f, 0x1a, 0x32, 0x35, 0x01, 0x40, 0x7a,
            0x40, 0x3c, 0x22, 0x11, 0xa8, 0x00, 0x00, 0x00, 0x01, 0x68, 0x1a, 0x34, 0xe3, 0xc8,
        ];

        let mut incomplete_single_payload_multi_nalu: Vec<u8> = vec![
            0x78, 0x00, 0x0f, 0x67, 0x42, 0xc0, 0x1f, 0x1a, 0x32, 0x35, 0x01, 0x40, 0x7a, 0x40,
            0x3c, 0x22, 0x11,
        ];

        let mut pkt = H264Packet::default();

        let result = pkt.depacketize(&mut []);
        assert!(result.is_err(), "Unmarshal did not fail on nil payload");

        let result = pkt.depacketize(&mut [0x00u8, 0x00][..]);
        assert!(
            result.is_err(),
            "Unmarshal accepted a packet that is too small for a payload and header"
        );

        let result = pkt.depacketize(&mut [0xFF, 0x00, 0x00][..]);
        assert!(
            result.is_err(),
            "Unmarshal accepted a packet with a NALU Type we don't handle"
        );

        let result = pkt.depacketize(incomplete_single_payload_multi_nalu.as_mut_slice());
        assert!(
            result.is_err(),
            "Unmarshal accepted a STAP-A packet with insufficient data"
        );

        let res = pkt.depacketize(single_payload.as_mut_slice())?;
        assert_eq!(
            res, single_payload_unmarshaled,
            "Unmarshaling a single payload shouldn't modify the payload"
        );

        let mut large_payload_result = vec![];
        for mut p in large_payload_packetized {
            let res = pkt.depacketize(p.as_mut_slice())?;
            large_payload_result.extend_from_slice(&res);
        }

        assert_eq!(
            large_payload_result, large_payload,
            "Failed to unmarshal a large payload"
        );

        let res = pkt.depacketize(single_payload_multi_nalu.as_mut_slice())?;
        assert_eq!(
            res, single_payload_multi_nalu_unmarshaled,
            "Failed to unmarshal a single packet with multiple NALUs"
        );

        Ok(())
    }
}
