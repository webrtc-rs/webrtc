use super::*;
use crate::goodbye::Goodbye;
use crate::payload_feedbacks::picture_loss_indication::PictureLossIndication;

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
    0x7b, 0x39, 0x63, 0x30, 0x30, 0x65, 0x62, 0x39, 0x32, 0x2d, 0x31, 0x61, 0x66, 0x62, 0x2d, 0x39,
    0x64, 0x34, 0x39, 0x2d, 0x61, 0x34, 0x37, 0x64, 0x2d, 0x39, 0x31, 0x66, 0x36, 0x34, 0x65, 0x65,
    0x65, 0x36, 0x39, 0x66, 0x35, 0x7d, // text="{9c00eb92-1afb-9d49-a47d-91f64eee69f5}"
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
    let mut short_header = Bytes::from_static(&[
        0x81, 0xc9, // missing type & len
    ]);
    let result = unmarshal(&mut short_header);
    assert!(result.is_err(), "missing type & len");
}

#[test]
fn test_bad_compound() {
    let mut bad_compound = Bytes::copy_from_slice(&REAL_PACKET[..34]);
    let result = unmarshal(&mut bad_compound);
    assert!(result.is_err(), "trailing data!");

    let mut bad_compound = Bytes::copy_from_slice(&REAL_PACKET[84..104]);
    let p = unmarshal(&mut bad_compound).expect("Error unmarshalling packet");
    let compound = CompoundPacket(p);

    // this should return an error,
    // it violates the "must start with RR or SR" rule
    match compound.validate() {
        Ok(_) => panic!("validation should return an error"),

        Err(err) => {
            let a = Error::BadFirstPacket;
            assert_eq!(
                Error::BadFirstPacket,
                err,
                "Unmarshal(badcompound) err={err:?}, want {a:?}",
            );
        }
    };

    let compound_len = compound.0.len();
    assert_eq!(
        compound_len, 2,
        "Unmarshal(badcompound) len={}, want {}",
        compound_len, 2
    );

    if compound.0[0].as_any().downcast_ref::<Goodbye>().is_none() {
        panic!("Unmarshal(badcompound), want Goodbye")
    }

    if compound.0[1]
        .as_any()
        .downcast_ref::<PictureLossIndication>()
        .is_none()
    {
        panic!("Unmarshal(badcompound), want PictureLossIndication")
    }
}

#[test]
fn test_valid_packet() {
    let cname = SourceDescription {
        chunks: vec![SourceDescriptionChunk {
            source: 1234,
            items: vec![SourceDescriptionItem {
                sdes_type: SdesType::SdesCname,
                text: Bytes::from_static(b"cname"),
            }],
        }],
    };

    let tests: Vec<(&str, CompoundPacket, Option<Error>)> = vec![
        (
            "no cname",
            CompoundPacket(vec![Box::<SenderReport>::default()]),
            Some(Error::MissingCname),
        ),
        (
            "SDES / no cname",
            CompoundPacket(vec![
                Box::<SenderReport>::default(),
                Box::<SourceDescription>::default(),
            ]),
            Some(Error::MissingCname),
        ),
        (
            "just SR",
            CompoundPacket(vec![
                Box::<SenderReport>::default(),
                Box::new(cname.to_owned()),
            ]),
            None,
        ),
        (
            "multiple SRs",
            CompoundPacket(vec![
                Box::<SenderReport>::default(),
                Box::<SenderReport>::default(),
                Box::new(cname.clone()),
            ]),
            Some(Error::PacketBeforeCname),
        ),
        (
            "just RR",
            CompoundPacket(vec![
                Box::<ReceiverReport>::default(),
                Box::new(cname.clone()),
            ]),
            None,
        ),
        (
            "multiple RRs",
            CompoundPacket(vec![
                Box::<ReceiverReport>::default(),
                Box::new(cname.clone()),
                Box::<ReceiverReport>::default(),
            ]),
            None,
        ),
        (
            "goodbye",
            CompoundPacket(vec![
                Box::<ReceiverReport>::default(),
                Box::new(cname),
                Box::<Goodbye>::default(),
            ]),
            None,
        ),
    ];

    for (name, packet, error) in tests {
        let result = packet.validate();
        assert_eq!(result.is_ok(), error.is_none());
        if let (Some(err), Err(got)) = (error, result) {
            assert_eq!(err, got, "Valid({name}) = {got:?}, want {err:?}");
        }
    }
}

#[test]
fn test_cname() {
    let cname = SourceDescription {
        chunks: vec![SourceDescriptionChunk {
            source: 1234,
            items: vec![SourceDescriptionItem {
                sdes_type: SdesType::SdesCname,
                text: Bytes::from_static(b"cname"),
            }],
        }],
    };

    let tests: Vec<(&str, CompoundPacket, Option<Error>, &str)> = vec![
        (
            "no cname",
            CompoundPacket(vec![Box::<SenderReport>::default()]),
            Some(Error::MissingCname),
            "",
        ),
        (
            "SDES / no cname",
            CompoundPacket(vec![
                Box::<SenderReport>::default(),
                Box::<SourceDescription>::default(),
            ]),
            Some(Error::MissingCname),
            "",
        ),
        (
            "just SR",
            CompoundPacket(vec![
                Box::<SenderReport>::default(),
                Box::new(cname.clone()),
            ]),
            None,
            "cname",
        ),
        (
            "multiple SRs",
            CompoundPacket(vec![
                Box::<SenderReport>::default(),
                Box::<SenderReport>::default(),
                Box::new(cname.clone()),
            ]),
            Some(Error::PacketBeforeCname),
            "",
        ),
        (
            "just RR",
            CompoundPacket(vec![
                Box::<ReceiverReport>::default(),
                Box::new(cname.clone()),
            ]),
            None,
            "cname",
        ),
        (
            "multiple RRs",
            CompoundPacket(vec![
                Box::<ReceiverReport>::default(),
                Box::<ReceiverReport>::default(),
                Box::new(cname.clone()),
            ]),
            None,
            "cname",
        ),
        (
            "goodbye",
            CompoundPacket(vec![
                Box::<ReceiverReport>::default(),
                Box::new(cname),
                Box::<Goodbye>::default(),
            ]),
            None,
            "cname",
        ),
    ];

    for (name, compound_packet, want_error, text) in tests {
        let err = compound_packet.validate();
        assert_eq!(err.is_err(), want_error.is_some());
        if let (Some(want), Err(err)) = (&want_error, err) {
            assert_eq!(*want, err, "Valid({name}) = {err:?}, want {want:?}");
        }

        let name_result = compound_packet.cname();
        assert_eq!(name_result.is_err(), want_error.is_some());

        match name_result {
            Ok(e) => {
                assert_eq!(e, text, "CNAME({name}) = {e:?}, want {text}",);
            }

            Err(err) => {
                if let Some(want) = &want_error {
                    assert_eq!(*want, err, "CNAME({name}) = {err:?}, want {want:?}");
                }
            }
        }
    }
}

#[test]
fn test_compound_packet_roundtrip() {
    let cname = SourceDescription {
        chunks: vec![SourceDescriptionChunk {
            source: 1234,
            items: vec![SourceDescriptionItem {
                sdes_type: SdesType::SdesCname,
                text: Bytes::from_static(b"cname"),
            }],
        }],
    };

    let tests = vec![
        (
            "goodbye",
            CompoundPacket(vec![
                Box::<ReceiverReport>::default(),
                Box::new(cname),
                Box::new(Goodbye {
                    sources: vec![1234],
                    ..Default::default()
                }),
            ]),
            None,
        ),
        (
            "no cname",
            CompoundPacket(vec![Box::<ReceiverReport>::default()]),
            Some(Error::MissingCname),
        ),
    ];

    for (name, packet, marshal_error) in tests {
        let result = packet.marshal();
        if let Some(err) = marshal_error {
            if let Err(got) = result {
                assert_eq!(err, got, "marshal {name} header: err = {got}, want {err}");
            } else {
                panic!("want error in test {name}");
            }
            continue;
        } else {
            assert!(result.is_ok(), "must no error in test {name}");
        }

        let data1 = result.unwrap();
        let c = CompoundPacket::unmarshal(&mut data1.clone())
            .unwrap_or_else(|_| panic!("unmarshal {name} error"));

        let data2 = c
            .marshal()
            .unwrap_or_else(|_| panic!("marshal {name} error"));

        assert_eq!(
            data1, data2,
            "Unmarshal(Marshal({name:?})) = {data1:?}, want {data2:?}"
        )
    }
}
