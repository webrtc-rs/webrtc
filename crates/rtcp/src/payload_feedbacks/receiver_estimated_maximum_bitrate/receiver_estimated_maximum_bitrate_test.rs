use super::*;

#[test]
fn test_receiver_estimated_maximum_bitrate_unmarshal() {
    let tests = vec![
        (
            "valid",
            Bytes::from_static(&[
                143u8, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 82, 69, 77, 66, 1, 26, 32, 223, 72, 116,
                237, 22,
            ]),
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 1,
                bitrate: 8927168,
                ssrcs: vec![1215622422],
            },
            None,
        ),
        (
            "Real data sent by Chrome while watching a 6Mb/s stream",
            // mantissa = []byte{26 & 3, 32, 223} = []byte{2, 32, 223} = 139487
            // exp = 26 >> 2 = 6
            // bitrate = 139487 * 2^6 = 139487 * 64 = 8927168 = 8.9 Mb/s
            Bytes::from_static(&[
                143, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 82, 69, 77, 66, 1, 26, 32, 223, 72, 116,
                237, 22,
            ]),
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 1,
                bitrate: 8927168,
                ssrcs: vec![1215622422],
            },
            None,
        ),
        (
            "Marshal a packet with the maximum possible bitrate.",
            // bitrate = 0xFFFFC00000000000
            // mantissa = 262143 = 0x3FFFF
            // exp = 46
            Bytes::from_static(&[
                143, 206, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 82, 69, 77, 66, 0, 187, 255, 255,
            ]),
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 0,
                bitrate: 0xFFFFC00000000000,
                ssrcs: vec![],
            },
            None,
        ),
        (
            "Marshal a packet with the overflowed bitrate.",
            // bitrate = 0xFFFFFFFFFFFFFFFF
            // mantissa = 0
            // exp = 47
            Bytes::from_static(&[
                143, 206, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 82, 69, 77, 66, 0, 188, 0, 0,
            ]),
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 0,
                bitrate: 0xFFFFFFFFFFFFFFFF,
                ssrcs: vec![],
            },
            None,
        ),
    ];

    for (name, data, want, want_error) in tests {
        let got = ReceiverEstimatedMaximumBitrate::unmarshal(&data);

        assert_eq!(
            got.is_err(),
            want_error.is_some(),
            "Unmarshal {} rr: err = {:?}, want {:?}",
            name,
            got,
            want_error
        );

        if let Some(err) = want_error {
            let got_err = got.err().unwrap();
            assert_eq!(
                got_err, err,
                "Unmarshal {} rr: err = {:?}, want {:?}",
                name, got_err, err,
            );
        } else {
            let actual = got.unwrap();
            assert_eq!(
                actual, want,
                "Unmarshal {} rr: got {:?}, want {:?}",
                name, actual, want
            );
        }
    }
}

#[test]
fn test_receiver_estimated_maximum_bitrate_roundtrip() {
    let tests: Vec<(
        &str,
        ReceiverEstimatedMaximumBitrate,
        Option<()>,
        Option<u64>,
    )> = vec![
        (
            "Real data sent by Chrome while watching a 6Mb/s stream",
            // mantissa = []byte{26 & 3, 32, 223} = []byte{2, 32, 223} = 139487
            // exp = 26 >> 2 = 6
            // bitrate = 139487 * 2^6 = 139487 * 64 = 8927168 = 8.9 Mb/s
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 1,
                bitrate: 8927168,
                ssrcs: vec![1215622422],
            },
            None,
            None,
        ),
        (
            "Marshal a packet with the maximum possible bitrate.",
            // bitrate = 0xFFFFC00000000000
            // mantissa = 262143 = 0x3FFFF
            // exp = 46
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 0,
                bitrate: 0xFFFFC00000000000,
                ssrcs: vec![],
            },
            None,
            None,
        ),
        (
            "Marshal a packet with the overflowed bitrate.",
            // bitrate = 0xFFFFFFFFFFFFFFFF
            // mantissa = 0
            // exp = 47
            ReceiverEstimatedMaximumBitrate {
                sender_ssrc: 0,
                bitrate: 0xFFFFFFFFFFFFFFFF,
                ssrcs: vec![],
            },
            None,
            Some(0xFFFFC00000000000u64),
        ),
    ];

    for (name, report, marshal_error, unmarshal_expected) in tests {
        let got = report.marshal();

        assert_eq!(
            got.is_ok(),
            marshal_error.is_none(),
            "Marshal {}: err = {:?}, want {:?}",
            name,
            got,
            marshal_error
        );

        let data = got.ok().unwrap();
        let actual = ReceiverEstimatedMaximumBitrate::unmarshal(&data)
            .expect(format!("Unmarshal {}", name).as_str());

        if let Some(expected_bitrate) = unmarshal_expected {
            assert_eq!(
                actual.bitrate, expected_bitrate,
                "{} round trip: got {:?}, want {:?}",
                name, actual.bitrate, expected_bitrate
            )
        } else {
            assert_eq!(
                actual, report,
                "{} header round trip: got {:?}, want {:?}",
                name, actual, report
            );
        }
    }
}
