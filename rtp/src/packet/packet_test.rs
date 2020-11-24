#[cfg(test)]
mod packet_test {
    use crate::{header, packet::Packet};
    use util::Error;

    #[test]
    fn test_basic() -> Result<(), Error> {
        let mut empty_bytes = vec![];

        let mut packet = Packet::default();
        let result = packet.unmarshal(&mut empty_bytes);

        if result.is_ok() {
            assert!(false, "Unmarshal did not error on zero length packet");
        }

        let mut raw_pkt = vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x00, 0x01,
            0x00, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let parsed_packet = Packet {
            header: header::Header {
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
                extensions: vec![header::Extension {
                    id: 0,
                    payload: vec![0xFF, 0xFF, 0xFF, 0xFF],
                }],

                payload_offset: 20,
                ..Default::default()
            },

            payload: vec![0x98, 0x36, 0xbe, 0x88, 0x9e],
            raw: raw_pkt.clone(),
        };

        let mut packet = Packet::default();
        packet.unmarshal(&mut raw_pkt)?;

        assert_eq!(
            packet, parsed_packet,
            "TestBasic unmarshal: got \n{:#?}, want \n{:#?}",
            packet, parsed_packet
        );

        assert_eq!(
            packet.marshal_size(),
            raw_pkt.len(),
            "wrong computed marshal size"
        );

        assert_eq!(
            packet.header.payload_offset, 20,
            "wrong payload offset: {} != {} ",
            packet.header.payload_offset, 20
        );

        let raw = packet.marshal()?;

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
        let mut missing_extension_pkt = vec![
            0x90, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82,
        ];

        let mut packet = Packet::default();
        let result = packet.unmarshal(&mut missing_extension_pkt);

        if result.is_ok() {
            assert!(
                false,
                "Unmarshal did not error on packet with missing extension data"
            );
        }

        let mut invalid_extension_length_pkt = vec![
            0x90, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x99, 0x99,
            0x99, 0x99,
        ];

        let mut packet = Packet::default();
        let result = packet.unmarshal(&mut invalid_extension_length_pkt);

        if result.is_ok() {
            assert!(
                false,
                "Unmarshal did not error on packet with invalid extension length"
            );
        }

        let mut packet = Packet {
            header: header::Header {
                extension: true,
                extension_profile: 3,
                extensions: vec![header::Extension {
                    id: 0,
                    payload: vec![0],
                }],
                ..Default::default()
            },

            payload: vec![],
            ..Default::default()
        };

        let result = packet.marshal();

        if result.is_ok() {
            assert!(
                false,
                "Marshal did not error on packet with invalid extension length"
            );
        }

        Ok(())
    }

    #[test]
    fn test_rfc8285_one_byte_extension() -> Result<(), Error> {
        let mut raw_pkt = vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x01, 0x50, 0xAA, 0x00, 0x00, 0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let mut packet = Packet::default();
        packet.unmarshal(&mut raw_pkt)?;

        let mut p = Packet {
            header: header::Header {
                marker: true,
                extension: true,
                extension_profile: crate::header::ExtensionProfile::OneByte.into(),
                extensions: vec![header::Extension {
                    id: 5,
                    payload: vec![0xAA],
                }],
                version: 2,
                payload_offset: 18,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                csrc: vec![],
                ..Default::default()
            },

            payload: raw_pkt[20..].to_vec(),
            raw: raw_pkt.clone(),
        };

        let dst = p.marshal()?;

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
        let mut raw_pkt = vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x01, 0x10, 0xAA, 0x20, 0xBB, // Payload
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let mut packet = Packet::default();
        packet.unmarshal(&mut raw_pkt)?;

        let ext1 = packet.header.get_extension(1);
        let ext1_expect = &[0xAA];
        if let Some(ext1) = ext1 {
            assert_eq!(ext1, ext1_expect);
        } else {
            assert!(false, "ext1 is none");
        }

        let ext2 = packet.header.get_extension(2);
        let ext2_expect = [0xBB];
        if let Some(ext2) = ext2 {
            assert_eq!(ext2, ext2_expect);
        } else {
            assert!(false, "ext2 is none");
        }

        // Test Marshal
        let mut p = Packet {
            header: header::Header {
                marker: true,
                extension: true,
                extension_profile: header::ExtensionProfile::OneByte.into(),
                extensions: vec![
                    header::Extension {
                        id: 1,
                        payload: vec![0xAA],
                    },
                    header::Extension {
                        id: 2,
                        payload: vec![0xBB],
                    },
                ],
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                csrc: vec![],
                ..Default::default()
            },

            payload: raw_pkt[20..].to_vec(),
            ..Default::default()
        };

        let dst: Vec<u8> = p.marshal()?;

        assert_eq!(dst, raw_pkt);

        Ok(())
    }

    #[test]
    fn test_rfc8285_one_byte_multiple_extensions_with_padding() {
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

        let mut raw_pkt: Vec<u8> = vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x00, 0x00, 0x33, 0xCC, 0xCC, 0xCC, 0xCC,
            // Payload
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let mut packet = Packet::default();

        packet
            .unmarshal(&mut raw_pkt)
            .expect("Error unmarshalling packets");

        let ext1 = packet
            .header
            .get_extension(1)
            .expect("Error getting header extension.");

        let ext1_expect: [u8; 1] = [0xAA];
        assert_eq!(ext1, ext1_expect);

        let ext2 = packet
            .header
            .get_extension(2)
            .expect("Error getting header extension.");

        let ext2_expect: [u8; 2] = [0xBB, 0xBB];
        assert_eq!(ext2, ext2_expect);

        let ext3 = packet
            .header
            .get_extension(3)
            .expect("Error getting header extension.");

        let ext3_expect: [u8; 4] = [0xCC, 0xCC, 0xCC, 0xCC];
        assert_eq!(ext3, ext3_expect);

        let mut dst_buf: [Vec<u8>; 2] = [vec![0u8; 1000], vec![0xFF; 1000]];

        let raw_pkg_marshal: [u8; 33] = [
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x33, 0xCC, 0xCC, 0xCC, 0xCC, 0x00,
            0x00, // padding is moved to the end by re-marshaling
            // Payload
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let checker = |name: &str, mut buf: &mut [u8], p: &mut Packet| {
            let n = p.marshal_to(&mut buf).expect("Error marshalling byte");

            assert_eq!(
                &buf[..n],
                &raw_pkg_marshal[..],
                "Marshalled fields are not equal for {}.",
                name
            );
        };

        checker("CleanBuffer", &mut dst_buf[0], &mut packet);
        checker("DirtyBuffer", &mut dst_buf[1], &mut packet);
    }

    //TODO: TestRFC8285OneByteMultipleExtensionsWithPadding
    //TODO: ...
    //TODO: TestRoundtrip
}

#[test]
fn test_rfc8285_one_byte_multiple_extensions_with_padding() {
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

    let raw_pkt: Vec<u8> = vec![
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x00, 0x00, 0x33, 0xCC, 0xCC, 0xCC, 0xCC,
        // Payload
        0x98, 0x36, 0xbe, 0x88, 0x9e,
    ];

    let packet = Packet::unmarshal(&mut BufReader::new(raw_pkt.as_slice()))
        .expect("Error unmarshalling packets");

    let ext1 = packet
        .header
        .get_extension(1)
        .expect("Error getting header extension.");

    let ext1_expect: [u8; 1] = [0xAA];
    assert_eq!(ext1, ext1_expect);

    let ext2 = packet
        .header
        .get_extension(2)
        .expect("Error getting header extension.");

    let ext2_expect: [u8; 2] = [0xBB, 0xBB];
    assert_eq!(ext2, ext2_expect);

    let ext3 = packet
        .header
        .get_extension(3)
        .expect("Error getting header extension.");

    let ext3_expect: [u8; 4] = [0xCC, 0xCC, 0xCC, 0xCC];
    assert_eq!(ext3, ext3_expect);

    let mut dst_buf: Vec<Vec<u8>> = vec![vec![0u8; 1000], vec![0xFF; 1000], vec![0xAA; 2]];

    let raw_pkg_marshal: [u8; 33] = [
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x33, 0xCC, 0xCC, 0xCC, 0xCC, 0x00, 0x00,
        // padding is moved to the end by re-marshaling
        // Payload
        0x98, 0x36, 0xbe, 0x88, 0x9e,
    ];

    let checker = |name: &str, buf: &mut Vec<u8>, p: &Packet| {
        {
            // NOTE: buf.as_mut_slice() won't increase buf size.
            // If buf size is not big enough, it will be silent and won't report error
            let mut writer = BufWriter::new(buf.as_mut_slice());
            p.marshal(&mut writer).expect("Error marshalling byte");
        }

        //println!("{:?}", &buf[..raw_pkg_marshal.len()]);

        assert_eq!(
            &buf[..p.size()],
            &raw_pkg_marshal[..],
            "Marshalled fields are not equal for {}.",
            name
        );
    };

    checker("CleanBuffer", &mut dst_buf[0], &packet);
    checker("DirtyBuffer", &mut dst_buf[1], &packet);

    {
        // NOTE: buf.as_mut_slice() won't increase buf size.
        // If buf size is not big enough, it will be silent and won't report error
        let mut writer = BufWriter::new(dst_buf[2].as_mut_slice());
        let result = packet.marshal(&mut writer);
        assert!(result.is_err());
    }
}

//TODO: ADD more tests in https://github.com/pion/rtp/blob/master/packet_test.go
//TODO: ...
//TODO: TestRoundtrip
