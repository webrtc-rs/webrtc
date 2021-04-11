use super::*;

#[test]
fn test_basic() -> Result<(), Error> {
    let empty_bytes = Bytes::new();
    let result = Packet::unmarshal(&empty_bytes);
    assert!(
        result.is_err(),
        "Unmarshal did not error on zero length packet"
    );

    let raw_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x00, 0x01, 0x00,
        0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);
    let parsed_packet = Packet {
        header: Header {
            version: 2,
            padding: false,
            extension: true,
            marker: true,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            extension_profile: 1,
            extensions: vec![Extension {
                id: 0,
                payload: Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]),
            }],
            ..Default::default()
        },
        payload: Bytes::from_static(&[0x98, 0x36, 0xbe, 0x88, 0x9e]),
    };

    let packet = Packet::unmarshal(&raw_pkt)?;
    assert_eq!(
        packet, parsed_packet,
        "TestBasic unmarshal: got {}, want {}",
        packet, parsed_packet
    );
    assert_eq!(
        packet.header.marshal_size(),
        20,
        "wrong computed header marshal size"
    );
    assert_eq!(
        packet.marshal_size(),
        raw_pkt.len(),
        "wrong computed marshal size"
    );

    let mut raw = BytesMut::new();
    let n = packet.marshal_to(&mut raw)?;
    assert_eq!(n, raw_pkt.len(), "wrong marshal size");

    assert_eq!(
        raw.len(),
        raw_pkt.len(),
        "wrong raw marshal size {} vs {}",
        raw.len(),
        raw_pkt.len()
    );
    assert_eq!(
        raw, raw_pkt,
        "TestBasic marshal: got {:?}, want {:?}",
        raw, raw_pkt
    );

    Ok(())
}

#[test]
fn test_extension() -> Result<(), Error> {
    let missing_extension_pkt = Bytes::from_static(&[
        0x90, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82,
    ]);
    let result = Packet::unmarshal(&missing_extension_pkt);
    assert!(
        result.is_err(),
        "Unmarshal did not error on packet with missing extension data"
    );

    let invalid_extension_length_pkt = Bytes::from_static(&[
        0x90, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x99, 0x99, 0x99,
        0x99,
    ]);
    let result = Packet::unmarshal(&invalid_extension_length_pkt);
    assert!(
        result.is_err(),
        "Unmarshal did not error on packet with invalid extension length"
    );

    let packet = Packet {
        header: Header {
            extension: true,
            extension_profile: 3,
            extensions: vec![Extension {
                id: 0,
                payload: Bytes::from_static(&[0]),
            }],
            ..Default::default()
        },
        payload: Bytes::from_static(&[]),
    };

    let mut raw = BytesMut::new();
    let result = packet.marshal_to(&mut raw);
    assert!(
        result.is_err(),
        "Marshal did not error on packet with invalid extension length"
    );

    Ok(())
}

#[test]
fn test_packet_marshal_unmarshal() -> Result<(), Error> {
    let pkt = Packet {
        header: Header {
            extension: true,
            csrc: vec![1, 2],
            extension_profile: EXTENSION_PROFILE_TWO_BYTE,
            extensions: vec![
                Extension {
                    id: 1,
                    payload: Bytes::from_static(&[3, 4]),
                },
                Extension {
                    id: 2,
                    payload: Bytes::from_static(&[5, 6]),
                },
            ],
            ..Default::default()
        },
        payload: Bytes::from_static(&[0xFFu8; 15]),
        ..Default::default()
    };
    let mut raw = BytesMut::new();
    let _ = pkt.marshal_to(&mut raw)?;

    let raw = raw.freeze();
    let p = Packet::unmarshal(&raw)?;

    assert_eq!(pkt, p);

    Ok(())
}

#[test]
fn test_rfc8285_one_byte_extension() -> Result<(), Error> {
    let raw_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x01, 0x50, 0xAA, 0x00, 0x00, 0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);
    Packet::unmarshal(&raw_pkt)?;

    let p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            extensions: vec![Extension {
                id: 5,
                payload: Bytes::from_static(&[0xAA]),
            }],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            ..Default::default()
        },
        payload: raw_pkt.slice(20..),
    };

    let mut dst = BytesMut::new();
    let _ = p.marshal_to(&mut dst)?;
    assert_eq!(dst, raw_pkt);

    Ok(())
}

#[test]
fn test_rfc8285one_byte_two_extension_of_two_bytes() -> Result<(), Error> {
    //  0                   1                   2                   3
    //  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |       0xBE    |    0xDE       |           length=1            |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |  ID   | L=0   |     data      |  ID   |  L=0  |   data...
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    let raw_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x01, 0x10, 0xAA, 0x20, 0xBB, // Payload
        0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);
    let p = Packet::unmarshal(&raw_pkt)?;

    let ext1 = p.header.get_extension(1);
    let ext1_expect = Bytes::from_static(&[0xAA]);
    if let Some(ext1) = ext1 {
        assert_eq!(ext1, ext1_expect);
    } else {
        assert!(false, "ext1 is none");
    }

    let ext2 = p.header.get_extension(2);
    let ext2_expect = Bytes::from_static(&[0xBB]);
    if let Some(ext2) = ext2 {
        assert_eq!(ext2, ext2_expect);
    } else {
        assert!(false, "ext2 is none");
    }

    // Test Marshal
    let p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            extensions: vec![
                Extension {
                    id: 1,
                    payload: Bytes::from_static(&[0xAA]),
                },
                Extension {
                    id: 2,
                    payload: Bytes::from_static(&[0xBB]),
                },
            ],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            ..Default::default()
        },
        payload: raw_pkt.slice(20..),
    };

    let mut dst = BytesMut::new();
    let _ = p.marshal_to(&mut dst)?;
    assert_eq!(dst, raw_pkt);

    Ok(())
}

#[test]
fn test_rfc8285_one_byte_multiple_extensions_with_padding() -> Result<(), Error> {
    //  0                   1                   2                   3
    //  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |       0xBE    |    0xDE       |           length=3            |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |  ID   | L=0   |     data      |  ID   |  L=1  |   data...
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //       ...data   |    0 (pad)    |    0 (pad)    |  ID   | L=3   |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |                          data                                 |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

    let raw_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x00, 0x00, 0x33, 0xCC, 0xCC, 0xCC, 0xCC,
        // Payload
        0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);

    let packet = Packet::unmarshal(&raw_pkt)?;
    let ext1 = packet
        .header
        .get_extension(1)
        .expect("Error getting header extension.");

    let ext1_expect = Bytes::from_static(&[0xAA]);
    assert_eq!(ext1, ext1_expect);

    let ext2 = packet
        .header
        .get_extension(2)
        .expect("Error getting header extension.");

    let ext2_expect = Bytes::from_static(&[0xBB, 0xBB]);
    assert_eq!(ext2, ext2_expect);

    let ext3 = packet
        .header
        .get_extension(3)
        .expect("Error getting header extension.");

    let ext3_expect = Bytes::from_static(&[0xCC, 0xCC, 0xCC, 0xCC]);
    assert_eq!(ext3, ext3_expect);

    let mut dst_buf: Vec<BytesMut> = vec![
        BytesMut::with_capacity(1000),
        BytesMut::with_capacity(1000),
        BytesMut::with_capacity(2),
    ];

    let raw_pkg_marshal = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x33, 0xCC, 0xCC, 0xCC, 0xCC, 0x00, 0x00,
        // padding is moved to the end by re-marshaling
        // Payload
        0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);

    let checker = |name: &str, buf: &mut BytesMut, p: &Packet| {
        let _ = p.marshal_to(buf).unwrap();
        let dst = buf.clone().freeze();
        assert_eq!(
            dst, raw_pkg_marshal,
            "Marshalled fields are not equal for {}.",
            name
        );
    };

    checker("CleanBuffer", &mut dst_buf[0], &packet);
    checker("DirtyBuffer", &mut dst_buf[1], &packet);

    let result = packet.marshal_to(&mut dst_buf[2]);
    assert!(result.is_ok());

    Ok(())
}

//TODO: ADD more tests in https://github.com/pion/rtp/blob/master/packet_test.go
//TODO: ...
//TODO: TestRoundtrip
