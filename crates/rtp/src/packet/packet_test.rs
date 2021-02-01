#[cfg(test)]
mod tests {

    use std::collections::hash_map;

    use crate::packet::*;

    #[test]
    fn test_basic() -> Result<(), RTPError> {
        let mut p = Packet::default();

        let result = p.unmarshal(&mut BytesMut::new());
        assert!(result.is_err());

        let raw_pkt = vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x00, 0x01,
            0x00, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let parsed_packet = Packet {
            header: Header {
                version: 2,
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
                    payload: vec![0xFF, 0xFF, 0xFF, 0xFF],
                }],
                payload_offset: 20,
                ..Default::default()
            },
            payload: raw_pkt[20..].into(),
            raw: raw_pkt[..].into(),
            ..Default::default()
        };

        // Unmarshal to the used Packet should work as well.
        p.unmarshal(&mut raw_pkt[..].into())?;
        assert_eq!(
            p, parsed_packet,
            "TestBasic unmarshal: got {}, want {}",
            p, parsed_packet
        );

        assert_eq!(
            p.marshal_size(),
            raw_pkt.len(),
            "wrong computed marshal size"
        );

        assert_eq!(
            p.header.payload_offset, 20,
            "wrong payload offset: {} != {}",
            p.header.payload_offset, 20
        );

        let raw = p.marshal()?;

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

        assert_eq!(
            p.header.payload_offset, 20,
            "wrong payload offset: {} != {}",
            p.header.payload_offset, 20
        );

        Ok(())
    }

    #[test]
    fn test_extension() -> Result<(), RTPError> {
        let missing_extension_pkt = vec![
            0x90, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82,
        ];

        let mut p = Packet::default();

        let result = p.unmarshal(&mut missing_extension_pkt[..].into());
        assert!(
            result.is_err(),
            "Unmarshal did not error on packet with missing extension data"
        );

        let invalid_extension_length_pkt = vec![
            0x90, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x99, 0x99,
            0x99, 0x99,
        ];

        let result = p.unmarshal(&mut invalid_extension_length_pkt[..].into());
        assert!(
            result.is_err(),
            "Unmarshal did not error on packet with invalid extension length"
        );

        let mut packet = Packet {
            header: Header {
                extension: true,
                extension_profile: 3,
                extensions: vec![Extension {
                    id: 0,
                    payload: vec![0],
                }],
                ..Default::default()
            },
            ..Default::default()
        };

        let result = packet.marshal();
        assert!(
            result.is_err(),
            "Marshal did not error on packet with invalid extension length"
        );

        Ok(())
    }

    #[test]
    fn test_rfc8285_one_byte_extension() -> Result<(), RTPError> {
        let raw_pkt = vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x01, 0x50, 0xAA, 0x00, 0x00, 0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let mut p = Packet::default();

        p.unmarshal(&mut raw_pkt[..].into())?;

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0xBEDE,
                extensions: vec![Extension {
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
            ..Default::default()
        };

        let dst_data = p.marshal()?;
        assert_eq!(dst_data, raw_pkt);

        Ok(())
    }

    #[test]
    fn test_rfc8285_one_byte_two_extension_of_two_bytes() -> Result<(), RTPError> {
        //  0                   1                   2                   3
        //  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |       0xBE    |    0xDE       |           length=1            |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |  ID   | L=0   |     data      |  ID   |  L=0  |   data...
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        let raw_pkt = vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x01, 0x10, 0xAA, 0x20, 0xBB, // Payload
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let mut p = Packet::default();

        p.unmarshal(&mut raw_pkt[..].into())?;

        let ext1 = p.header.get_extension(1);
        let ext1_expect = &[0xAAu8][..];
        assert_eq!(
            ext1,
            Some(ext1_expect),
            "Extension has incorrect data, Got: {:?}, expected {:?}",
            ext1,
            Some(ext1_expect),
        );

        let ext2 = p.header.get_extension(2);
        let ext2_expect = &[0xBBu8][..];
        assert_eq!(
            ext2,
            Some(ext2_expect),
            "Extension has incorrect data, Got: {:?}, expected {:?}",
            ext2,
            Some(ext2_expect),
        );

        // Test Marshal
        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0xBEDE,
                extensions: vec![
                    Extension {
                        id: 1,
                        payload: vec![0xAA],
                    },
                    Extension {
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
            raw: raw_pkt[..].into(),
        };

        let dst_data = p.marshal()?;

        assert_eq!(
            dst_data, raw_pkt,
            "Marshal failed raw: {:?} \n original data: {:?}",
            raw_pkt, dst_data
        );

        Ok(())
    }

    #[test]
    fn test_rfc8285_one_byte_multiple_extensions_with_padding() -> Result<(), RTPError> {
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
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x00, 0x00, 0x33, 0xCC, 0xCC, 0xCC, 0xCC,
            // Payload
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let mut p = Packet::default();

        p.unmarshal(&mut raw_pkt[..].into())?;

        let ext1 = p.header.get_extension(1);

        let ext1_expect = &[0xAAu8][..];
        assert_eq!(
            ext1,
            Some(ext1_expect),
            "Extension has incorrect data. Got: {:?}, Expected: {:?}",
            ext1,
            ext1_expect
        );

        let ext2 = p.header.get_extension(2);

        let ext2_expect = &[0xBBu8, 0xBB][..];
        assert_eq!(
            ext2,
            Some(ext2_expect),
            "Extension has incorrect data. Got: {:?}, Expected: {:?}",
            ext2,
            ext2_expect
        );

        let ext3 = p.header.get_extension(3);

        let ext3_expect = &[0xCCu8, 0xCC, 0xCC, 0xCC][..];
        assert_eq!(
            ext3,
            Some(ext3_expect),
            "Extension has incorrect data. Got: {:?}, Expected: {:?}",
            ext3,
            ext3_expect
        );

        let dst_buf: Vec<Vec<u8>> = vec![vec![0u8; 1000], vec![0xFF; 1000], vec![0xAA; 2]];

        let raw_pkg_marshal: [u8; 33] = [
            // padding is moved to the end by re-marshaling
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x33, 0xCC, 0xCC, 0xCC, 0xCC, 0x00, 0x00,
            // Payload
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let checker = |name: &str, buf: &mut BytesMut, p: &mut Packet| -> Result<(), RTPError> {
            let size = p.marshal_to(buf)?;
            //println!("{:?}", &buf[..raw_pkg_marshal.len()]);

            assert_eq!(
                &buf[..size],
                &raw_pkg_marshal[..],
                "Marshalled fields are not equal for {}.",
                name
            );

            Ok(())
        };

        checker("CleanBuffer", &mut dst_buf[0][..].into(), &mut p)?;
        checker("DirtyBuffer", &mut dst_buf[1][..].into(), &mut p)
    }

    fn test_rfc_8285_one_byte_multiple_extension() -> Result<(), RTPError> {
        //  0                   1                   2                   3
        //  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |       0xBE    |    0xDE       |           length=3            |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |  ID=1 | L=0   |     data      |  ID=2 |  L=1  |   data...
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //       ...data   |  ID=3 | L=3   |           data...
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //             ...data             |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        let raw_pkt: BytesMut = [
            0x90u8, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x33, 0xCC, 0xCC, 0xCC, 0xCC, 0x00, 0x00,
            // Payload
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ][..]
            .into();

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0xBEDE,
                extensions: vec![
                    Extension {
                        id: 1,
                        payload: vec![0xAA],
                    },
                    Extension {
                        id: 2,
                        payload: vec![0xBB, 0xBB],
                    },
                    Extension {
                        id: 3,
                        payload: vec![0xCC, 0xCC],
                    },
                ],
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: raw_pkt[28..].into(),
            raw: raw_pkt[..].into(),
        };

        let dst_data = p.marshal()?;
        assert_eq!(
            dst_data, raw_pkt,
            "Marshal failed raw \nMarshaled:\n{:?}\nrawPkt:\n{:?}",
            dst_data, raw_pkt,
        );

        Ok(())
    }

    fn test_rfc_8285_two_byte_extension() -> Result<(), RTPError> {
        let mut p = Packet::default();

        let raw_pkt: BytesMut = [
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x10, 0x00,
            0x00, 0x07, 0x05, 0x18, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
            0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
            0x00, 0x00, 0x98, 0x36, 0xbe, 0x88, 0x9e,
        ][..]
            .into();

        p.unmarshal(&mut raw_pkt.clone())?;

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0x1000,
                extensions: vec![Extension {
                    id: 5,
                    payload: vec![
                        0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
                        0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
                    ],
                }],
                version: 2,
                payload_offset: 42,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: raw_pkt[44..].into(),
            raw: raw_pkt.clone(),
        };

        let dst_data = p.marshal()?;
        assert_eq!(
            dst_data, raw_pkt,
            "Marshal failed raw \nMarshaled:\n{:?}\nrawPkt:\n{:?}",
            dst_data, raw_pkt
        );
        Ok(())
    }

    fn test_rfc8285_two_byte_multiple_extension_with_padding() -> Result<(), RTPError> {
        let mut p = Packet::default();

        // 0                   1                   2                   3
        // 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |       0x10    |    0x00       |           length=3            |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |      ID=1     |     L=0       |     ID=2      |     L=1       |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |       data    |    0 (pad)    |       ID=3    |      L=4      |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |                          data                                 |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

        let raw_pkt = [
            0x90u8, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x10, 0x00,
            0x00, 0x03, 0x01, 0x00, 0x02, 0x01, 0xBB, 0x00, 0x03, 0x04, 0xCC, 0xCC, 0xCC, 0xCC,
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        p.unmarshal(&mut raw_pkt[..].into())?;

        let ext = p.header.get_extension(1);
        let ext_expect: &[u8] = &[];
        assert_eq!(
            ext,
            Some(ext_expect),
            "Extension has incorrect data. Got: {:?}, Expected: {:?}",
            ext,
            ext_expect
        );

        let ext = p.header.get_extension(2);
        let ext_expect: &[u8] = &[0xBB];
        assert_eq!(
            ext,
            Some(ext_expect),
            "Extension has incorrect data. Got: {:?}, Expected: {:?}",
            ext,
            ext_expect
        );

        let ext = p.header.get_extension(3);
        let ext_expect: &[u8] = &[0xCC, 0xCC, 0xCC, 0xCC];
        assert_eq!(
            ext,
            Some(ext_expect),
            "Extension has incorrect data. Got: {:?}, Expected: {:?}",
            ext,
            ext_expect
        );

        Ok(())
    }

    fn test_rfc8285_two_byte_multiple_extension_with_large_extension() -> Result<(), RTPError> {
        // 0                   1                   2                   3
        // 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |       0x10    |    0x00       |           length=3            |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |      ID=1     |     L=0       |     ID=2      |     L=1       |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |       data    |       ID=3    |      L=17      |    data...
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //                            ...data...
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //                            ...data...
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //                            ...data...
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //                            ...data...                           |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

        let raw_pkt = [
            0x90u8, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x10, 0x00,
            0x00, 0x06, 0x01, 0x00, 0x02, 0x01, 0xBB, 0x03, 0x11, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
            // Payload
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0x1000,
                extensions: vec![
                    Extension {
                        id: 1,
                        payload: vec![],
                    },
                    Extension {
                        id: 2,
                        payload: vec![0xBB],
                    },
                    Extension {
                        id: 3,
                        payload: vec![
                            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
                            0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
                        ],
                    },
                ],
                version: 2,
                payload_offset: 40,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: raw_pkt[40..].into(),
            raw: raw_pkt[..].into(),
        };

        let dst_data = p.marshal()?;
        assert_eq!(
            dst_data,
            raw_pkt[..],
            "Marshal failed raw \nMarshaled: {:?}, \nraw_pkt:{:?}",
            dst_data,
            raw_pkt
        );

        Ok(())
    }

    fn test_rfc8285_get_extension_returns_nil_when_extension_disabled() -> Result<(), RTPError> {
        let payload = [
            // Payload
            0x98u8, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let p = Packet {
            header: Header {
                marker: true,
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload.into(),
            ..Default::default()
        };

        let res = p.header.get_extension(1);
        assert!(
            res.is_none(),
            "Should return none on get_extension when header extension is false"
        );

        Ok(())
    }

    fn test_rfc8285_del_extension() -> Result<(), RTPError> {
        let payload = [
            // Payload
            0x98u8, 0x36, 0xbe, 0x88, 0x9e,
        ];
        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0xBEDE,
                extensions: vec![Extension {
                    id: 1,
                    payload: vec![0xAA],
                }],
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload.into(),
            ..Default::default()
        };

        let ext = p.header.get_extension(1);
        assert!(ext.is_some(), "Extension should exist");

        p.header.del_extension(1)?;

        let ext = p.header.get_extension(1);
        assert!(ext.is_none(), "Extension should not exist");

        let err = p.header.del_extension(1);
        assert!(
            err.is_err(),
            "Should return error when deleting extension that doesnt exist"
        );

        Ok(())
    }

    fn test_rfc8285_get_extension_ids() {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0xBEDE,
                extensions: vec![
                    Extension {
                        id: 1,
                        payload: vec![0xAA],
                    },
                    Extension {
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
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        let ids = p.header.get_extension_ids();
        assert!(!ids.is_empty(), "Extenstions should exist");

        assert_eq!(
            ids.len(),
            p.header.extensions.len(),
            "The number of IDs should be equal to the number of extensions, want={}, hanve{}",
            ids.len(),
            p.header.extensions.len()
        );

        for id in ids {
            let ext = p.header.get_extension(id);
            assert!(ext.is_some(), "Extension should exist for id: {}", id)
        }
    }

    fn test_rfc8285_get_extension_ids_return_empty_when_extension_disabled() {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let p = Packet {
            header: Header {
                marker: true,
                extension: false,
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        let ids = p.header.get_extension_ids();
        assert!(ids.is_empty(), "Extenstions should not exist");
    }

    fn test_rfc8285_del_extension_returns_error_when_extenstions_disabled() {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: false,
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        let ids = p.header.del_extension(1);
        assert!(
            ids.is_err(),
            "Should return error on del_extension when header extension field is false"
        );
    }

    fn test_rfc8285_one_byte_set_extension_should_enable_extension_when_adding() {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: false,
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        let extension = [0xAAu8, 0xAA];
        let result = p.header.set_extension(1, &extension[..].into());
        assert!(result.is_ok(), "Error setting extension");

        assert_eq!(p.header.extension, true, "Extension should be set to true");
        assert_eq!(
            p.header.extension_profile, 0xBEDE,
            "Extension profile should be set to 0xBEDE"
        );
        assert_eq!(
            p.header.extensions.len(),
            1,
            "Extensions len should be set to 1"
        );
        assert_eq!(
            p.header.get_extension(1),
            Some(extension[..].into()),
            "Extension value is not set"
        )
    }

    fn test_rfc8285_set_extension_should_set_correct_extension_profile_for_16_byte_extension() {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: false,
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        let extension = [
            0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
            0xAA, 0xAA,
        ];

        let res = p.header.set_extension(1, &extension[..].into());
        assert!(res.is_ok(), "Error setting extension");

        assert_eq!(
            p.header.extension_profile, 0xBEDE,
            "Extension profile should be 0xBEDE"
        );
    }

    fn test_rfc8285_set_extension_should_update_existing_extension() -> Result<(), RTPError> {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0xBEDE,
                extensions: vec![Extension {
                    id: 1,
                    payload: vec![0xAA],
                }],
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        assert_eq!(
            p.header.get_extension(1),
            Some([0xAA][..].into()),
            "Extension value not initialized properly"
        );

        let extension = [0xBBu8];
        p.header.set_extension(1, &extension[..].into())?;

        assert_eq!(
            p.header.get_extension(1),
            extension[..].into(),
            "Extension value was not set"
        );

        Ok(())
    }

    fn test_rfc8285_one_byte_set_extension_should_error_when_invalid_id_provided() {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0xBEDE,
                extensions: vec![Extension {
                    id: 1,
                    payload: vec![0xAA],
                }],
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        assert!(
            p.header.set_extension(0, &[0xBBu8][..].into()).is_err(),
            "set_extension did not error on invalid id"
        );
        assert!(
            p.header.set_extension(15, &[0xBBu8][..].into()).is_err(),
            "set_extension did not error on invalid id"
        );
    }

    fn test_rfc8285_one_byte_extension_terminate_processing_when_reserved_id_encountered(
    ) -> Result<(), RTPError> {
        let mut p = Packet::default();

        let reserved_id_pkt = [
            0x90u8, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x01, 0xF0, 0xAA, 0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        p.unmarshal(&mut reserved_id_pkt[..].into())?;

        assert_eq!(
            p.header.extensions.len(),
            0,
            "Extension should be empty for invalid ID"
        );

        let payload = &reserved_id_pkt[17..];
        assert_eq!(
            p.payload,
            payload.to_vec(),
            "p.payload must be same as payload"
        );

        Ok(())
    }

    fn test_rfc8285_one_byte_set_extension_should_error_when_payload_too_large() {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0xBEDE,
                extensions: vec![Extension {
                    id: 1,
                    payload: vec![0xAA],
                }],
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        let res = p.header.set_extension(
            1,
            &([
                0xBBu8, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB,
            ][..]
                .into()),
        );

        assert!(
            res.is_err(),
            "set_extension did not error on too large payload"
        );
    }

    fn test_rfc8285_two_bytes_set_extension_should_enable_extension_when_adding(
    ) -> Result<(), RTPError> {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0xBEDE,
                version: 2,
                payload_offset: 31,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        let extension = [
            0xAAu8, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
            0xAA, 0xAA, 0xAA,
        ];

        p.header.set_extension(1, &extension[..].into())?;

        assert_eq!(p.header.extension, true, "Extension should be set to true");
        assert_eq!(
            p.header.extension_profile, 0x1000,
            "Extension profile should be set to 0xBEDE"
        );
        assert_eq!(
            p.header.extensions.len(),
            1,
            "Extensions should be set to 1"
        );
        assert_eq!(
            p.header.get_extension(1),
            Some(&extension[..]),
            "Extension value is not set"
        );

        Ok(())
    }

    fn test_rfc8285_two_byte_set_extension_should_update_existing_extension() -> Result<(), RTPError>
    {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0x1000,
                extensions: vec![Extension {
                    id: 1,
                    payload: vec![0xAA],
                }],
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        assert_eq!(
            p.header.get_extension(1),
            Some([0xAA][..].into()),
            "Extension value not initialized properly"
        );

        let extension = [
            0xBBu8, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB,
        ];

        p.header.set_extension(1, &extension[..].into())?;

        assert_eq!(p.header.get_extension(1), Some(&extension[..]));

        Ok(())
    }

    fn test_rfc8285_two_byte_set_extension_should_error_when_payload_too_large() {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0xBEDE,
                extensions: vec![Extension {
                    id: 1,
                    payload: vec![0xAA],
                }],
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        let res = p.header.set_extension(
            1,
            &[
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
                0xBB, 0xBB, 0xBB, 0xBB,
            ][..]
                .into(),
        );

        assert!(
            res.is_err(),
            "Set extension did not error on too large payload"
        );
    }

    fn test_rfc3550_set_extension_should_error_when_non_zero() -> Result<(), RTPError> {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0x1111,
                extensions: vec![Extension {
                    id: 1,
                    payload: vec![0xAA],
                }],
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        p.header.set_extension(0, &[0xBB][..].into())?;
        let res = p.header.get_extension(0);
        assert_eq!(
            res,
            Some([0xBB][..].into()),
            "p.get_extenstion returned incorrect value"
        );

        Ok(())
    }

    fn test_rfc3550_set_extension_should_error_when_setting_non_zero_id() {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: 0x1111,
                version: 2,
                payload_offset: 26,
                payload_type: 96,
                sequence_number: 27023,
                timestamp: 3653407706,
                ssrc: 476325762,
                ..Default::default()
            },
            payload: payload[..].into(),
            ..Default::default()
        };

        let res = p.header.set_extension(1, &[0xBB][..].into());
        assert!(res.is_err(), "set_extension did not error on invalid id");
    }

    struct Cases {
        input: Vec<u8>,
        err: Option<RTPError>,
    }

    fn test_unmarshal_error_handling() {
        let mut cases = hash_map::HashMap::new();

        cases.insert(
            "ShortHeader",
            Cases {
                input: vec![
                    0x80, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, // timestamp
                    0x1c, 0x64, 0x27, // SSRC (one byte missing)
                ],
                err: Some(RTPError::HeaderSizeInsufficient),
            },
        );

        cases.insert(
            "MissingCSRC",
            Cases {
                input: vec![
                    0x81, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, // timestamp
                    0x1c, 0x64, 0x27, 0x82, // SSRC
                ],
                err: Some(RTPError::HeaderSizeInsufficient),
            },
        );

        cases.insert(
            "MissingExtension",
            Cases {
                input: vec![
                    0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, // timestamp
                    0x1c, 0x64, 0x27, 0x82, // SSRC
                ],
                err: Some(RTPError::HeaderSizeInsufficientForExtension),
            },
        );

        cases.insert(
            "MissingExtensionData",
            Cases {
                input: vec![
                    0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, // timestamp
                    0x1c, 0x64, 0x27, 0x82, // SSRC
                    0xBE, 0xDE, 0x00,
                    0x03, // specified to have 3 extensions, but actually not
                ],
                err: Some(RTPError::HeaderSizeInsufficientForExtension),
            },
        );

        cases.insert(
            "MissingExtensionDataPayload",
            Cases {
                input: vec![
                    0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, // timestamp
                    0x1c, 0x64, 0x27, 0x82, // SSRC
                    0xBE, 0xDE, 0x00, 0x01, // have 1 extension
                    0x12,
                    0x00, // length of the payload is expected to be 3, but actually have only 1
                ],
                err: Some(RTPError::HeaderSizeInsufficientForExtension),
            },
        );

        for (name, test_case) in cases.drain() {
            let mut h = Header::default();
            let result = h.unmarshal(&mut test_case.input[..].into());

            assert_eq!(
                result.err(),
                test_case.err,
                "Expected :{:?}, found: {:?} for testcase {}",
                test_case.err,
                result.err(),
                name
            )
        }
    }

    fn test_round_trip() -> Result<(), RTPError> {
        let raw_pkt = vec![
            0x00u8, 0x10, 0x23, 0x45, 0x12, 0x34, 0x45, 0x67, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11,
            0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        ];

        let payload = raw_pkt[12..].to_vec();

        let mut p = Packet::default();

        p.unmarshal(&mut raw_pkt[..].into())?;

        assert_eq!(
            raw_pkt, p.raw,
            "p.Raw must be same as raw_pkt.\n p.raw: {:?},\nraw_pkt: {:?}",
            p.raw, raw_pkt
        );
        assert_eq!(
            payload, p.payload,
            "p.payload must be same as payload.\n p.payload: {:?},\nraw_pkt: {:?}",
            p.payload, payload
        );

        let buf = p.marshal()?;

        assert_eq!(
            raw_pkt, buf,
            "buf must be the same as raw_pkt. \n buf: {:?},\nraw_pkt: {:?}",
            buf, raw_pkt,
        );
        assert_eq!(
            raw_pkt, p.raw,
            "p.raw must be the same as raw_pkt. \n p.raw: {:?},\nraw_pkt: {:?}",
            p.raw, raw_pkt,
        );
        assert_eq!(
            payload, p.payload,
            "p.payload must be the same as payload. \n payload: {:?},\np.payload: {:?}",
            payload, p.payload,
        );

        Ok(())
    }
}
