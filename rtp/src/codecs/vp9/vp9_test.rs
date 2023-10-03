use super::*;

#[test]
fn test_vp9_packet_unmarshal() -> Result<()> {
    let tests = vec![
        (
            "Empty",
            Bytes::from_static(&[]),
            Vp9Packet::default(),
            Bytes::new(),
            Some(Error::ErrShortPacket),
        ),
        (
            "NonFlexible",
            Bytes::from_static(&[0x00, 0xAA]),
            Vp9Packet::default(),
            Bytes::from_static(&[0xAA]),
            None,
        ),
        (
            "NonFlexiblePictureID",
            Bytes::from_static(&[0x80, 0x02, 0xAA]),
            Vp9Packet {
                i: true,
                picture_id: 0x02,
                ..Default::default()
            },
            Bytes::from_static(&[0xAA]),
            None,
        ),
        (
            "NonFlexiblePictureIDExt",
            Bytes::from_static(&[0x80, 0x81, 0xFF, 0xAA]),
            Vp9Packet {
                i: true,
                picture_id: 0x01FF,
                ..Default::default()
            },
            Bytes::from_static(&[0xAA]),
            None,
        ),
        (
            "NonFlexiblePictureIDExt_ShortPacket0",
            Bytes::from_static(&[0x80, 0x81]),
            Vp9Packet::default(),
            Bytes::new(),
            Some(Error::ErrShortPacket),
        ),
        (
            "NonFlexiblePictureIDExt_ShortPacket1",
            Bytes::from_static(&[0x80]),
            Vp9Packet::default(),
            Bytes::new(),
            Some(Error::ErrShortPacket),
        ),
        (
            "NonFlexibleLayerIndicePictureID",
            Bytes::from_static(&[0xA0, 0x02, 0x23, 0x01, 0xAA]),
            Vp9Packet {
                i: true,
                l: true,
                picture_id: 0x02,
                tid: 0x01,
                sid: 0x01,
                d: true,
                tl0picidx: 0x01,
                ..Default::default()
            },
            Bytes::from_static(&[0xAA]),
            None,
        ),
        (
            "FlexibleLayerIndicePictureID",
            Bytes::from_static(&[0xB0, 0x02, 0x23, 0x01, 0xAA]),
            Vp9Packet {
                f: true,
                i: true,
                l: true,
                picture_id: 0x02,
                tid: 0x01,
                sid: 0x01,
                d: true,
                ..Default::default()
            },
            Bytes::from_static(&[0x01, 0xAA]),
            None,
        ),
        (
            "NonFlexibleLayerIndicePictureID_ShortPacket0",
            Bytes::from_static(&[0xA0, 0x02, 0x23]),
            Vp9Packet::default(),
            Bytes::new(),
            Some(Error::ErrShortPacket),
        ),
        (
            "NonFlexibleLayerIndicePictureID_ShortPacket1",
            Bytes::from_static(&[0xA0, 0x02]),
            Vp9Packet::default(),
            Bytes::new(),
            Some(Error::ErrShortPacket),
        ),
        (
            "FlexiblePictureIDRefIndex",
            Bytes::from_static(&[0xD0, 0x02, 0x03, 0x04, 0xAA]),
            Vp9Packet {
                i: true,
                p: true,
                f: true,
                picture_id: 0x02,
                pdiff: vec![0x01, 0x02],
                ..Default::default()
            },
            Bytes::from_static(&[0xAA]),
            None,
        ),
        (
            "FlexiblePictureIDRefIndex_TooManyPDiff",
            Bytes::from_static(&[0xD0, 0x02, 0x03, 0x05, 0x07, 0x09, 0x10, 0xAA]),
            Vp9Packet::default(),
            Bytes::new(),
            Some(Error::ErrTooManyPDiff),
        ),
        (
            "FlexiblePictureIDRefIndexNoPayload",
            Bytes::from_static(&[0xD0, 0x02, 0x03, 0x04]),
            Vp9Packet {
                i: true,
                p: true,
                f: true,
                picture_id: 0x02,
                pdiff: vec![0x01, 0x02],
                ..Default::default()
            },
            Bytes::from_static(&[]),
            None,
        ),
        (
            "FlexiblePictureIDRefIndex_ShortPacket0",
            Bytes::from_static(&[0xD0, 0x02, 0x03]),
            Vp9Packet::default(),
            Bytes::new(),
            Some(Error::ErrShortPacket),
        ),
        (
            "FlexiblePictureIDRefIndex_ShortPacket1",
            Bytes::from_static(&[0xD0, 0x02]),
            Vp9Packet::default(),
            Bytes::new(),
            Some(Error::ErrShortPacket),
        ),
        (
            "FlexiblePictureIDRefIndex_ShortPacket2",
            Bytes::from_static(&[0xD0]),
            Vp9Packet::default(),
            Bytes::new(),
            Some(Error::ErrShortPacket),
        ),
        (
            "ScalabilityStructureResolutionsNoPayload",
            Bytes::from_static(&[
                0x0A,
                (1 << 5) | (1 << 4), // NS:1 Y:1 G:0
                (640 >> 8) as u8,
                (640 & 0xff) as u8,
                (360 >> 8) as u8,
                (360 & 0xff) as u8,
                (1280 >> 8) as u8,
                (1280 & 0xff) as u8,
                (720 >> 8) as u8,
                (720 & 0xff) as u8,
            ]),
            Vp9Packet {
                b: true,
                v: true,
                ns: 1,
                y: true,
                g: false,
                ng: 0,
                width: vec![640, 1280],
                height: vec![360, 720],
                ..Default::default()
            },
            Bytes::new(),
            None,
        ),
        (
            "ScalabilityStructureNoPayload",
            Bytes::from_static(&[
                0x0A,
                (1 << 5) | (1 << 3), // NS:1 Y:0 G:1
                2,
                (1 << 4),            // T:0 U:1 R:0 -
                (2 << 5) | (1 << 2), // T:2 U:0 R:1 -
                33,
            ]),
            Vp9Packet {
                b: true,
                v: true,
                ns: 1,
                y: false,
                g: true,
                ng: 2,
                pgtid: vec![0, 2],
                pgu: vec![true, false],
                pgpdiff: vec![vec![], vec![33]],
                ..Default::default()
            },
            Bytes::new(),
            None,
        ),
    ];

    for (name, b, pkt, expected, err) in tests {
        let mut p = Vp9Packet::default();

        if let Some(expected) = err {
            if let Err(actual) = p.depacketize(&b) {
                assert_eq!(
                    expected, actual,
                    "{name}: expected {expected}, but got {actual}"
                );
            } else {
                panic!("{name}: expected error, but got passed");
            }
        } else {
            let payload = p.depacketize(&b)?;
            assert_eq!(pkt, p, "{name}: expected {pkt:?}, but got {p:?}");
            assert_eq!(payload, expected);
        }
    }

    Ok(())
}

#[test]
fn test_vp9_payloader_payload() -> Result<()> {
    let mut r0 = 8692;
    let mut rands = vec![];
    for _ in 0..10 {
        rands.push(vec![(r0 >> 8) as u8 | 0x80, (r0 & 0xFF) as u8]);
        r0 += 1;
    }

    let tests = vec![
        ("NilPayload", vec![Bytes::new()], 100, vec![]),
        ("SmallMTU", vec![Bytes::from(vec![0x00, 0x00])], 1, vec![]),
        (
            "NegativeMTU",
            vec![Bytes::from(vec![0x00, 0x00])],
            0,
            vec![],
        ),
        (
            "OnePacket",
            vec![Bytes::from(vec![0x01, 0x02])],
            10,
            vec![Bytes::from(vec![
                0x9C,
                rands[0][0],
                rands[0][1],
                0x01,
                0x02,
            ])],
        ),
        (
            "TwoPackets",
            vec![Bytes::from(vec![0x01, 0x02])],
            4,
            vec![
                Bytes::from(vec![0x98, rands[0][0], rands[0][1], 0x01]),
                Bytes::from(vec![0x94, rands[0][0], rands[0][1], 0x02]),
            ],
        ),
        (
            "ThreePackets",
            vec![Bytes::from(vec![0x01, 0x02, 0x03])],
            4,
            vec![
                Bytes::from(vec![0x98, rands[0][0], rands[0][1], 0x01]),
                Bytes::from(vec![0x90, rands[0][0], rands[0][1], 0x02]),
                Bytes::from(vec![0x94, rands[0][0], rands[0][1], 0x03]),
            ],
        ),
        (
            "TwoFramesFourPackets",
            vec![Bytes::from(vec![0x01, 0x02, 0x03]), Bytes::from(vec![0x04])],
            5,
            vec![
                Bytes::from(vec![0x98, rands[0][0], rands[0][1], 0x01, 0x02]),
                Bytes::from(vec![0x94, rands[0][0], rands[0][1], 0x03]),
                Bytes::from(vec![0x9C, rands[1][0], rands[1][1], 0x04]),
            ],
        ),
    ];

    for (name, bs, mtu, expected) in tests {
        let mut pck = Vp9Payloader {
            initial_picture_id_fn: Some(Arc::new(|| -> u16 { 8692 })),
            ..Default::default()
        };

        let mut actual = vec![];
        for b in &bs {
            actual.extend(pck.payload(mtu, b)?);
        }
        assert_eq!(actual, expected, "{name}: Payloaded packet");
    }

    //"PictureIDOverflow"
    {
        let mut pck = Vp9Payloader {
            initial_picture_id_fn: Some(Arc::new(|| -> u16 { 8692 })),
            ..Default::default()
        };
        let mut p_prev = Vp9Packet::default();
        for i in 0..0x8000 {
            let res = pck.payload(4, &Bytes::from_static(&[0x01]))?;
            let mut p = Vp9Packet::default();
            p.depacketize(&res[0])?;

            if i > 0 {
                if p_prev.picture_id == 0x7FFF {
                    assert_eq!(
                        p.picture_id, 0,
                        "Picture ID next to 0x7FFF must be 0, got {}",
                        p.picture_id
                    );
                } else if p_prev.picture_id + 1 != p.picture_id {
                    panic!(
                        "Picture ID next must be incremented by 1: {} -> {}",
                        p_prev.picture_id, p.picture_id,
                    );
                }
            }

            p_prev = p;
        }
    }

    Ok(())
}

#[test]
fn test_vp9_partition_head_checker_is_partition_head() -> Result<()> {
    let vp9 = Vp9Packet::default();

    //"SmallPacket"
    assert!(
        !vp9.is_partition_head(&Bytes::new()),
        "Small packet should not be the head of a new partition"
    );

    //"NormalPacket"
    assert!(
        vp9.is_partition_head(&Bytes::from_static(&[0x18, 0x00, 0x00])),
        "VP9 RTP packet with B flag should be head of a new partition"
    );
    assert!(
        !vp9.is_partition_head(&Bytes::from_static(&[0x10, 0x00, 0x00])),
        "VP9 RTP packet without B flag should not be head of a new partition"
    );

    Ok(())
}
