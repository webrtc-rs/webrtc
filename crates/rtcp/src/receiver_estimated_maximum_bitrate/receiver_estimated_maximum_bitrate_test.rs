#[cfg(test)]
mod test {
    use crate::receiver_estimated_maximum_bitrate::*;

    #[test]
    fn test_receiver_estimated_maximum_bitrate_unmarshal() {
        let tests: Vec<(
            &str,
            Vec<u8>,
            ReceiverEstimatedMaximumBitrate,
            Result<(), Error>,
        )> = vec![(
            "valid",
            vec![
                143u8, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 82, 69, 77, 66, 1, 26, 32, 223, 72, 116,
                237, 22,
            ],
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 1,
                bitrate: 8927168,
                ssrcs: vec![1215622422],
            },
            Ok(()),
        )];

        for (name, data, want, want_error) in tests {
            let mut packet = ReceiverEstimatedMaximumBitrate::default();

            let result = packet.unmarshal(&mut data[..].into());

            assert_eq!(
                result, want_error,
                "Unmarshal {} header: err = {:?}, want {:?}",
                name, result, want_error
            );

            assert_eq!(
                packet, want,
                "Unmarshal {} header: got {:?}, want {:?}",
                name, result, want_error
            );
        }
    }

    #[test]
    fn test_receiver_estimated_maximum_bitrate_marshal() {
        let tests: Vec<(
            &str,
            Vec<u8>,
            ReceiverEstimatedMaximumBitrate,
            Result<(), Error>,
        )> = vec![(
            "valid",
            vec![
                143u8, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 82, 69, 77, 66, 1, 26, 32, 223, 72, 116,
                237, 22,
            ],
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 1,
                bitrate: 8927168,
                ssrcs: vec![1215622422],
            },
            Ok(()),
        )];

        for (name, data, want, want_error) in tests {
            let output = want.marshal();

            assert_eq!(
                output.clone().err(),
                want_error.clone().err(),
                "Marshal {} header: err = {:?}, want {:?}",
                name,
                output,
                want_error
            );

            match output {
                Ok(e) => {
                    assert_eq!(
                        &e[..],
                        &data[..],
                        "Bytes not equal for {} bytes: {:?}, want = {:?}",
                        name,
                        &e[..],
                        &data[..]
                    )
                }

                Err(_) => continue,
            }
        }
    }

    #[test]
    fn test_receiver_estimated_maximum_bitrate_truncate() {
        let input = [
            143u8, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 82, 69, 77, 66, 1, 26, 32, 223, 72, 116, 237,
            22,
        ];

        // Make sure that we're truncating the bitrate correctly.
        // For the above example, we have:

        // mantissa = 139487
        // exp = 6
        // bitrate = 8927168

        let mut packet = ReceiverEstimatedMaximumBitrate::default();

        packet
            .unmarshal(&mut input[..].into())
            .expect("Unmarshal: Unexpected error");

        assert_eq!(8927168u64, packet.bitrate, "Invalid bitrate");

        // Just verify marshal produces the same input.
        let output = packet.marshal().expect("Marshal: Unexpected error");

        assert_eq!(
            output.to_vec(),
            input.to_vec(),
            "Invalid bytes, expected = {:?} found {:?}",
            output,
            input
        );

        // If we subtract the bitrate by 1, we'll round down a lower mantissa
        packet.bitrate -= 1;

        // bitrate = 8927167
        // mantissa = 139486
        // exp = 6

        let mut output = packet
            .marshal()
            .expect("Marshal: unexpected error on truncating bitrate");

        assert_ne!(
            output.to_vec(),
            input.to_vec(),
            "Invalid bytes on truncating bitrate, expected = {:?} found = {:?}",
            output,
            input
        );

        // Which if we actually unmarshal again, we'll find that it's actually decreased by 63 (which is exp)
        // mantissa = 139486
        // exp = 6
        // bitrate = 8927104

        packet
            .unmarshal(&mut output)
            .expect("Unmarshal: Error on unmarshalling truncated bitrate bytes");

        assert_eq!(8927104u64, packet.bitrate)
    }

    #[test]
    fn test_receiver_estimated_maximum_bitrate_overflow() {
        // Marshal a packet with the maximum possible bitrate.
        let mut packet = ReceiverEstimatedMaximumBitrate {
            bitrate: 0xFFFFFFFFFFFFFFFF,
            ..Default::default()
        };

        // bitrate = 0xFFFFFFFFFFFFFFFF
        // mantissa = 262143 = 0x3FFFF
        // exp = 46

        let expected = [
            143u8, 206, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 82, 69, 77, 66, 0, 187, 255, 255,
        ];

        let mut output = packet.marshal().expect("Error marsahalling packets");

        assert_eq!(
            output.to_vec(),
            expected.to_vec(),
            "Unexpected bytes, want = {:?} have {:?}",
            output,
            expected
        );

        // mantissa = 262143
        // exp = 46
        // bitrate = 0xFFFFC00000000000

        // We actually can't represent the full uint64.
        // This is because the lower 46 bits are all 0s.

        packet
            .unmarshal(&mut output)
            .expect("Error unmarshalling bytes");

        assert_eq!(0xFFFFC00000000000, packet.bitrate, "Invalid bitrate");

        let output = packet.marshal().expect("Error marshalling packets");

        assert_eq!(
            output.to_vec(),
            expected.to_vec(),
            "Invalid bytes on marshalling"
        );

        // Finally, try unmarshalling one number higher than we can handle
        // It's debatable if the bitrate should have all lower 48 bits set.
        // I think it's better because uint64 overflow is easier to notice/debug.
        // And it's not like this class can actually ensure Marshal/Unmarshal are mirrored.
        let input = [
            143u8, 206, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 82, 69, 77, 66, 0, 188, 0, 0,
        ];

        packet
            .unmarshal(&mut input[..].into())
            .expect("Error marshalling bytes one number higher");

        assert_eq!(
            0xFFFFFFFFFFFFFFFF, packet.bitrate,
            "Invalid bytes on marshalling one number higher"
        )
    }
}
