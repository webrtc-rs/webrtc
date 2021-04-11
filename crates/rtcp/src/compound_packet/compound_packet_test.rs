#[cfg(test)]
mod test {
    use crate::compound_packet::*;
    use crate::{
        error::Error, goodbye, packet, receiver_report, sender_report, source_description,
    };

    // An RTCP packet from a packet dump
    const REAL_PACKET: [u8; 116] = [
        // Receiver Report (offset=0)
        0x81, 0xc9, 0x0, 0x7, // v=2, p=0, count=1, RR, len=7
        0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
        0xbc, 0x5e, 0x9a, 0x40, // ssrc=0xbc5e9a40
        0x0, 0x0, 0x0, 0x0, // fracLost=0, totalLost=0
        0x0, 0x0, 0x46, 0xe1, // lastSeq=0x46e1
        0x0, 0x0, 0x1, 0x11, // jitter=273
        0x9, 0xf3, 0x64, 0x32, // lsr=0x9f36432
        0x0, 0x2, 0x4a, 0x79, // delay=150137
        // Source Description (offset=32)
        0x81, 0xca, 0x0, 0xc, // v=2, p=0, count=1, SDES, len=12
        0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
        0x1, 0x26, // CNAME, len=38
        0x7b, 0x39, 0x63, 0x30, 0x30, 0x65, 0x62, 0x39, 0x32, 0x2d, 0x31, 0x61, 0x66, 0x62, 0x2d,
        0x39, 0x64, 0x34, 0x39, 0x2d, 0x61, 0x34, 0x37, 0x64, 0x2d, 0x39, 0x31, 0x66, 0x36, 0x34,
        0x65, 0x65, 0x65, 0x36, 0x39, 0x66, 0x35,
        0x7d, // text="{9c00eb92-1afb-9d49-a47d-91f64eee69f5}"
        0x0, 0x0, 0x0, 0x0, // END + padding
        // Goodbye (offset=84)
        0x81, 0xcb, 0x0, 0x1, // v=2, p=0, count=1, BYE, len=1
        0x90, 0x2f, 0x9e, 0x2e, // source=0x902f9e2e
        0x81, 0xce, 0x0, 0x2, // Picture Loss Indication (offset=92)
        0x90, 0x2f, 0x9e, 0x2e, // sender=0x902f9e2e
        0x90, 0x2f, 0x9e, 0x2e, // media=0x902f9e2e
        0x85, 0xcd, 0x0, 0x2, // RapidResynchronizationRequest (offset=104)
        0x90, 0x2f, 0x9e, 0x2e, // sender=0x902f9e2e
        0x90, 0x2f, 0x9e, 0x2e, // media=0x902f9e2e
    ];

    #[test]
    fn test_read_eof() {
        let short_header = vec![
            0x81, 0xc9, // missing type & len
        ]
        .as_slice()
        .into();

        let result = packet::unmarshal(short_header);
        assert!(result.is_err(), "missing type & len");
    }

    #[test]
    fn test_bad_compound() {
        let bad_compound = REAL_PACKET[..34].into();
        let result = packet::unmarshal(bad_compound);
        assert!(result.is_err(), "trailing data!");

        let bad_compound = REAL_PACKET[84..104].into();

        let packets = packet::unmarshal(bad_compound).expect("Error unmarshalling packet");

        let compound = CompoundPacket(packets);

        // this should return an error,
        // it violates the "must start with RR or SR" rule
        match compound.validate() {
            Ok(_) => panic!("validation should return an error"),

            Err(e) => {
                let a = Error::BadFirstPacket;
                assert_eq!(
                    e,
                    Error::BadFirstPacket,
                    "Unmarshal(badcompound) err={:?}, want {:?}",
                    e,
                    a,
                );
            }
        };

        let compound_len = compound.0.len();
        assert_eq!(
            compound_len, 2,
            "Unmarshal(badcompound) len={}, want {}",
            compound_len, 2
        );

        if let None = compound.0[0]
            .as_any()
            .downcast_ref::<crate::goodbye::Goodbye>()
        {
            panic!("Unmarshal(badcompound), want Goodbye")
        }

        if let None = compound.0[1]
            .as_any()
            .downcast_ref::<crate::picture_loss_indication::PictureLossIndication>()
        {
            panic!("Unmarshal(badcompound), want PictureLossIndication")
        }
    }

    #[test]
    fn test_valid_packet() {
        let cname = source_description::SourceDescription {
            chunks: vec![source_description::SourceDescriptionChunk {
                source: 1234,
                items: vec![source_description::SourceDescriptionItem {
                    sdes_type: source_description::SdesType::SdesCname,
                    text: "cname".to_owned(),
                }],
            }],
        };

        let tests = vec![
            (
                "no cname",
                CompoundPacket(vec![Box::new(sender_report::SenderReport::default())]),
                Err(Error::MissingCname),
            ),
            (
                "SDES / no cname",
                CompoundPacket(vec![
                    Box::new(sender_report::SenderReport::default()),
                    Box::new(source_description::SourceDescription::default()),
                ]),
                Err(Error::MissingCname),
            ),
            (
                "just SR",
                CompoundPacket(vec![
                    Box::new(sender_report::SenderReport::default()),
                    Box::new(cname.to_owned()),
                ]),
                Ok(()),
            ),
            (
                "multiple SRs",
                CompoundPacket(vec![
                    Box::new(sender_report::SenderReport::default()),
                    Box::new(sender_report::SenderReport::default()),
                    Box::new(cname.clone()),
                ]),
                Err(Error::PacketBeforeCname),
            ),
            (
                "just RR",
                CompoundPacket(vec![
                    Box::new(receiver_report::ReceiverReport::default()),
                    Box::new(cname.clone()),
                ]),
                Ok(()),
            ),
            (
                "multiple RRs",
                CompoundPacket(vec![
                    Box::new(receiver_report::ReceiverReport::default()),
                    Box::new(cname.clone()),
                    Box::new(receiver_report::ReceiverReport::default()),
                ]),
                Ok(()),
            ),
            (
                "goodbye",
                CompoundPacket(vec![
                    Box::new(receiver_report::ReceiverReport::default()),
                    Box::new(cname.clone()),
                    Box::new(goodbye::Goodbye::default()),
                ]),
                Ok(()),
            ),
        ];

        for (name, packet, error) in tests {
            let result = packet.validate();
            assert_eq!(
                result, error,
                "Valid({}) = {:?}, want {:?}",
                name, result, error
            );
        }
    }

    #[test]
    fn test_cname() {
        let cname = source_description::SourceDescription {
            chunks: vec![source_description::SourceDescriptionChunk {
                source: 1234,
                items: vec![source_description::SourceDescriptionItem {
                    sdes_type: SdesType::SdesCname,
                    text: "cname".to_string(),
                }],
            }],
        };

        let tests = vec![
            (
                "no cname",
                CompoundPacket(vec![Box::new(SenderReport::default())]),
                Some(Error::MissingCname),
                "",
            ),
            (
                "SDES / no cname",
                CompoundPacket(vec![
                    Box::new(SenderReport::default()),
                    Box::new(source_description::SourceDescription::default()),
                ]),
                Some(Error::MissingCname),
                "",
            ),
            (
                "just SR",
                CompoundPacket(vec![
                    Box::new(SenderReport::default()),
                    Box::new(cname.clone()),
                ]),
                None,
                "cname",
            ),
            (
                "multiple SRs",
                CompoundPacket(vec![
                    Box::new(SenderReport::default()),
                    Box::new(SenderReport::default()),
                    Box::new(cname.clone()),
                ]),
                Some(Error::PacketBeforeCname),
                "",
            ),
            (
                "just RR",
                CompoundPacket(vec![
                    Box::new(ReceiverReport::default()),
                    Box::new(cname.clone()),
                ]),
                None,
                "cname",
            ),
            (
                "multiple RRs",
                CompoundPacket(vec![
                    Box::new(ReceiverReport::default()),
                    Box::new(ReceiverReport::default()),
                    Box::new(cname.clone()),
                ]),
                None,
                "cname",
            ),
            (
                "goodbye",
                CompoundPacket(vec![
                    Box::new(ReceiverReport::default()),
                    Box::new(cname.clone()),
                    Box::new(goodbye::Goodbye::default()),
                ]),
                None,
                "cname",
            ),
        ];

        for (name, compound_packet, want_error, text) in tests {
            let err = compound_packet.validate();

            assert_eq!(
                err.clone().err(),
                want_error,
                "Valid({}) = {:?}, want {:?}",
                name,
                err.err(),
                want_error
            );

            let name_result = compound_packet.cname();

            assert_eq!(
                name_result.clone().err(),
                want_error,
                "CNAME({}) = {:?}, want {:?}",
                name,
                name_result.err(),
                want_error
            );

            match name_result {
                Ok(e) => {
                    assert_eq!(
                        e.as_str(),
                        text,
                        "CNAME({}) = {}, want {}",
                        name,
                        e.as_str(),
                        text,
                    );
                }

                Err(_) => continue,
            }
        }
    }

    #[test]
    fn test_compound_packet_roundtrip() {
        let cname = source_description::SourceDescription {
            chunks: vec![source_description::SourceDescriptionChunk {
                source: 1234,
                items: vec![source_description::SourceDescriptionItem {
                    sdes_type: SdesType::SdesCname,
                    text: "cname".to_string(),
                }],
            }],
        };

        let tests = vec![
            (
                "goodbye",
                CompoundPacket(vec![
                    Box::new(ReceiverReport::default()),
                    Box::new(cname.clone()),
                    Box::new(goodbye::Goodbye::default()),
                ]),
                None,
            ),
            (
                "no cname",
                CompoundPacket(vec![Box::new(ReceiverReport::default())]),
                Some(Error::MissingCname),
            ),
        ];

        for (name, packet, marshal_error) in tests {
            let result = packet.marshal();
            if let Some(err) = marshal_error {
                if let Err(got) = result {
                    assert_eq!(
                        got, err,
                        "marshal {} header: err = {}, want {}",
                        name, got, err
                    );
                } else {
                    assert!(false, "want error in test {}", name);
                }
                continue;
            } else {
                assert!(result.is_ok(), "must no error in test {}", name);
            }

            let mut c = CompoundPacket::default();

            let data1 = result.unwrap();

            c.unmarshal(data1.clone())
                .expect("Unmarshall should be nil");

            let data2 = packet.marshal().expect("Marshal should be nil");

            assert_eq!(
                data1, data2,
                "Unmarshal(Marshal({:?})) = {:?}, want {:?}",
                name, data1, data2
            )
        }
    }
}
