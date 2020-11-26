#[cfg(test)]
mod packet_test {
    use crate::{header, packet::Packet};
    use header::{Extension, ExtensionProfile, Header};
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

    #[test]
    fn test_rfc_8285_one_byte_multiple_extensions() {
        //  0                   1                   2                   3
        //  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |       0xBE    |    0xDE       |           length=3            |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |  ID=1 | L=0   |     data      |  ID=2 |  L=1  |   data...     |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //       ...data   |  ID=3 | L=3   |           data...
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //             ...data             |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

        let raw_pkt = vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x33, 0xCC, 0xCC, 0xCC, 0xCC, 0x00, 0x00,
            // Payload
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: ExtensionProfile::OneByte.into(),
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
                        payload: vec![0xCC, 0xCC, 0xCC, 0xCC],
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
            payload: raw_pkt[28..].to_vec(),
            raw: raw_pkt.clone(),
        };

        let dst_data = p.marshal().expect("Error marshalling packet.");

        assert_eq!(dst_data[..], raw_pkt[..]);
    }

    #[test]
    fn test_rfc_8285_two_byte_extension() {
        let mut raw_pkt = vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x10, 0x00,
            0x00, 0x07, 0x05, 0x18, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
            0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
            0x00, 0x00, 0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let mut p = Packet::default();

        p.unmarshal(&mut raw_pkt)
            .expect("Error unmarshalling packet");

        let mut p = Packet {
            header: header::Header {
                marker: true,
                extension: true,
                extension_profile: ExtensionProfile::TwoByte.into(),
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
            payload: raw_pkt[44..].to_vec(),
            raw: raw_pkt.clone(),
        };

        let dst_data = p.marshal().expect("Error marshalling packet");

        assert_eq!(dst_data[..], raw_pkt[..]);
    }

    #[test]
    fn test_rfc_8285_two_byte_multiple_extension_with_padding() {
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

        let mut p = Packet::default();

        let mut raw_pkt = vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x10, 0x00,
            0x00, 0x03, 0x01, 0x00, 0x02, 0x01, 0xBB, 0x00, 0x03, 0x04, 0xCC, 0xCC, 0xCC, 0xCC,
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        p.unmarshal(&mut raw_pkt)
            .expect("Error unmarshalling packet");

        match p.header.get_extension(1) {
            Some(e) => {
                assert_eq!(*e, []);
            }

            None => panic!("Header gave an empty extension"),
        }

        match p.header.get_extension(2) {
            Some(e) => {
                assert_eq!(*e, [0xBB]);
            }

            None => panic!("Header gave an empty extension"),
        }

        match p.header.get_extension(3) {
            Some(e) => {
                assert_eq!(*e, [0xCC, 0xCC, 0xCC, 0xCC]);
            }

            None => panic!("Header gave an empty extension"),
        }
    }

    #[test]
    fn test_rfc_8285_two_byte_multiple_extension_with_large_extension() {
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

        let raw_pkt = vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x10, 0x00,
            0x00, 0x06, 0x01, 0x00, 0x02, 0x01, 0xBB, 0x03, 0x11, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
            // Payload
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: ExtensionProfile::TwoByte.into(),
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
            payload: raw_pkt[40..].to_vec(),
            raw: raw_pkt.clone(),
        };

        let dst_data = p.marshal().expect("Error marshalling packet");

        assert_eq!(dst_data[..], raw_pkt[..]);
    }

    #[test]
    fn test_rfc_8285_get_extension_returns_nil_when_extension_disabled() {
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

            payload: payload.to_vec(),
            ..Default::default()
        };

        let val = p.header.get_extension(1);
        if val.is_some() {
            panic!("Should return a none value on get extension when self.extension is false");
        }
    }

    #[test]
    fn test_rfc_8285_del_extension() {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let mut p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: ExtensionProfile::OneByte.into(),
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

            payload: payload.to_vec(),
            ..Default::default()
        };

        p.header.get_extension(1).expect("Extension should exist");

        p.header
            .del_extension(1)
            .expect("Should successfully delete extension");

        if p.header.get_extension(1).is_some() {
            panic!("Extension should not exist");
        }

        p.header
            .del_extension(1)
            .expect_err("Should return an error when deleting extension that does'nt exist");
    }

    #[test]
    fn test_rfc_8285_get_extension_ids() {
        let payload = [0x98u8, 0x36, 0xbe, 0x88, 0x9e];

        let p = Packet {
            header: Header {
                marker: true,
                extension: true,
                extension_profile: ExtensionProfile::OneByte.into(),
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

            payload: payload.to_vec(),
            ..Default::default()
        };

        let ids = p.header.get_extension_ids();

        if ids.is_empty() {
            panic!("Extesnions should exist");
        }

        if ids.len() != p.header.extensions.len() {
            panic!(
                "The number of IDS should be equal to the number of extensions, want={}, got={}",
                p.header.extensions.len(),
                ids.len()
            );
        }

        for id in ids {
            p.header.get_extension(id).expect("Extension should exist");
        }
    }

    #[test]
    fn test_rfc_8285_get_extension_ids_returns_error_when_extensions_disabled() {
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

            payload: payload.to_vec(),
            ..Default::default()
        };

        let ids = p.header.get_extension_ids();

        if !ids.is_empty() {
            panic!(
                "Should return empty on get extension ids when Header Extensions variable is empty"
            );
        }
    }

    #[test]
    fn test_rfc_8285_del_extension_returns_error_when_extensions_disabled() {
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

            payload: payload.to_vec(),
            ..Default::default()
        };

        p.header
            .del_extension(1)
            .expect_err("Should return error on delete extension when Header Extension is false");
    }

    #[test]
    fn test_rfc_8285_one_byte_set_extension_should_enable_extension_when_adding() {
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

            payload: payload.to_vec(),
            ..Default::default()
        };

        let extension = vec![0xAAu8, 0xAA];

        p.header
            .set_extension(1, &extension)
            .expect("Error setting extension");

        if p.header.extension != true {
            panic!("Extension should be set to true");
        }

        if p.header.extension_profile != ExtensionProfile::OneByte.into() {
            panic!("Extension profile should be set to One Byte")
        }

        if p.header.extensions.len() != 1 {
            panic!("Extensions should be set to 1")
        }

        let ext = p
            .header
            .get_extension(1)
            .expect("Get extension should not be nil");

        assert_eq!(ext[..], extension[..]);
    }

    #[test]
    fn test_rfc_8285_one_byte_set_extension_should_set_correct_extension_profile_for_16_byte_extension(
    ) {
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

            payload: payload.to_vec(),
            ..Default::default()
        };

        let extension = vec![
            0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
            0xAA, 0xAA,
        ];

        p.header
            .set_extension(1, &extension)
            .expect("Error setting extension");

        if p.header.extension_profile != ExtensionProfile::OneByte.into() {
            panic!("Extension profile should be set to One Byte");
        }
    }
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
