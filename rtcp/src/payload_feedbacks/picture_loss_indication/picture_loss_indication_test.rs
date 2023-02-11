use bytes::Bytes;

use super::*;

#[test]
fn test_picture_loss_indication_unmarshal() {
    let tests = vec![
        (
            "valid",
            Bytes::from_static(&[
                0x81, 0xce, 0x00, 0x02, // v=2, p=0, FMT=1, PSFB, len=1
                0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
            ]),
            PictureLossIndication {
                sender_ssrc: 0x0,
                media_ssrc: 0x4bc4fcb4,
            },
            None,
        ),
        (
            "packet too short",
            Bytes::from_static(&[0x81, 0xce, 0x00, 0x00]),
            PictureLossIndication::default(),
            Some(Error::PacketTooShort),
        ),
        (
            "invalid header",
            Bytes::from_static(&[
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ]),
            PictureLossIndication::default(),
            Some(Error::BadVersion),
        ),
        (
            "wrong type",
            Bytes::from_static(&[
                0x81, 0xc9, 0x00, 0x02, // v=2, p=0, FMT=1, RR, len=1
                0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
            ]),
            PictureLossIndication::default(),
            Some(Error::WrongType),
        ),
        (
            "wrong fmt",
            Bytes::from_static(&[
                0x82, 0xc9, 0x00, 0x02, // v=2, p=0, FMT=2, RR, len=1
                0x00, 0x00, 0x00, 0x00, // ssrc=0x0
                0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
            ]),
            PictureLossIndication::default(),
            Some(Error::WrongType),
        ),
    ];

    for (name, mut data, want, want_error) in tests {
        let got = PictureLossIndication::unmarshal(&mut data);

        assert_eq!(
            got.is_err(),
            want_error.is_some(),
            "Unmarshal {name} rr: err = {got:?}, want {want_error:?}"
        );

        if let Some(err) = want_error {
            let got_err = got.err().unwrap();
            assert_eq!(
                err, got_err,
                "Unmarshal {name} rr: err = {got_err:?}, want {err:?}",
            );
        } else {
            let actual = got.unwrap();
            assert_eq!(
                actual, want,
                "Unmarshal {name} rr: got {actual:?}, want {want:?}"
            );
        }
    }
}

#[test]
fn test_picture_loss_indication_roundtrip() {
    let tests: Vec<(&str, PictureLossIndication, Option<Error>)> = vec![
        (
            "valid",
            PictureLossIndication {
                sender_ssrc: 1,
                media_ssrc: 2,
            },
            None,
        ),
        (
            "also valid",
            PictureLossIndication {
                sender_ssrc: 5000,
                media_ssrc: 6000,
            },
            None,
        ),
    ];

    for (name, want, want_error) in tests {
        let got = want.marshal();

        assert_eq!(
            got.is_ok(),
            want_error.is_none(),
            "Marshal {name}: err = {got:?}, want {want_error:?}"
        );

        if let Some(err) = want_error {
            let got_err = got.err().unwrap();
            assert_eq!(
                err, got_err,
                "Unmarshal {name} rr: err = {got_err:?}, want {err:?}",
            );
        } else {
            let mut data = got.ok().unwrap();
            let actual = PictureLossIndication::unmarshal(&mut data)
                .unwrap_or_else(|_| panic!("Unmarshal {name}"));

            assert_eq!(
                actual, want,
                "{name} round trip: got {actual:?}, want {want:?}"
            )
        }
    }
}

#[test]
fn test_picture_loss_indication_unmarshal_header() -> Result<()> {
    let tests = vec![(
        "valid header",
        Bytes::from_static(&[
            0x81u8, 0xce, 0x00, 0x02, // v=2, p=0, FMT=1, PSFB, len=1
            0x00, 0x00, 0x00, 0x00, // ssrc=0x0
            0x4b, 0xc4, 0xfc, 0xb4, // ssrc=0x4bc4fcb4
        ]),
        Header {
            count: FORMAT_PLI,
            packet_type: PacketType::PayloadSpecificFeedback,
            length: PLI_LENGTH as u16,
            ..Default::default()
        },
    )];

    for (name, mut data, header) in tests {
        let pli = PictureLossIndication::unmarshal(&mut data)?;

        assert_eq!(
            pli.header(),
            header,
            "Unmarshal header {} rr: got {:?}, want {:?}",
            name,
            pli.header(),
            header
        );
    }

    Ok(())
}
