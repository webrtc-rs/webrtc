use super::*;

use std::io::BufWriter;

use util::Error;

use crate::goodbye::*;
use crate::receiver_report::*;
use crate::sender_report::*;

lazy_static! {
// An RTCP packet from a packet dump
static ref REAL_PACKET:Vec<u8> = vec![
    // Receiver Report (offset=0)
    // v=2, p=0, count=1, RR, len=7
    0x81, 0xc9, 0x0, 0x7, // ssrc=0x902f9e2e
    0x90, 0x2f, 0x9e, 0x2e, // ssrc=0xbc5e9a40
    0xbc, 0x5e, 0x9a, 0x40, // fracLost=0, totalLost=0
    0x0, 0x0, 0x0, 0x0, // lastSeq=0x46e1
    0x0, 0x0, 0x46, 0xe1, // jitter=273
    0x0, 0x0, 0x1, 0x11, // lsr=0x9f36432
    0x9, 0xf3, 0x64, 0x32, // delay=150137
    0x0, 0x2, 0x4a, 0x79,
    // Source Description (offset=32)
    // v=2, p=0, count=1, SDES, len=12
    0x81, 0xca, 0x0, 0xc, // ssrc=0x902f9e2e
    0x90, 0x2f, 0x9e, 0x2e, // CNAME, len=38
    0x1, 0x26, // text="{9c00eb92-1afb-9d49-a47d-91f64eee69f5}"
    0x7b, 0x39, 0x63, 0x30, 0x30, 0x65, 0x62, 0x39, 0x32, 0x2d, 0x31, 0x61, 0x66, 0x62, 0x2d,
    0x39, 0x64, 0x34, 0x39, 0x2d, 0x61, 0x34, 0x37, 0x64, 0x2d, 0x39, 0x31, 0x66, 0x36, 0x34,
    0x65, 0x65, 0x65, 0x36, 0x39, 0x66, 0x35, 0x7d, // END + padding
    0x0, 0x0, 0x0, 0x0,
    // Goodbye (offset=84)
    // v=2, p=0, count=1, BYE, len=1
    0x81, 0xcb, 0x0, 0x1, // source=0x902f9e2e
    0x90, 0x2f, 0x9e, 0x2e, // Picture Loss Indication (offset=92)
    0x81, 0xce, 0x0, 0x2, // sender=0x902f9e2e
    0x90, 0x2f, 0x9e, 0x2e, // media=0x902f9e2e
    0x90, 0x2f, 0x9e, 0x2e, // RapidResynchronizationRequest (offset=104)
    0x85, 0xcd, 0x0, 0x2, // sender=0x902f9e2e
    0x90, 0x2f, 0x9e, 0x2e, // media=0x902f9e2e
    0x90, 0x2f, 0x9e, 0x2e,
];
}

#[test]
fn test_read_eof() -> Result<(), Error> {
    let short_header = vec![
        0x81, 0xc9, // missing type & len
    ];
    let result = unmarshal(&short_header);
    assert!(result.is_err(), "missing type & len");

    Ok(())
}

#[test]
fn test_bad_compound() -> Result<(), Error> {
    let bad_compound = &REAL_PACKET[..34];
    let result = unmarshal(bad_compound);
    assert!(result.is_err(), "trailing data!");

    let bad_compound = &REAL_PACKET[84..104];
    let packet = unmarshal(bad_compound)?;
    let result = match packet {
        Packet::CompoundPacket(p) => p.validate(),
        _ => Ok(()),
    };
    if let Err(got) = result {
        let want = ERR_BAD_FIRST_PACKET.clone();
        assert_eq!(
            got, want,
            "Unmarshal(badcompound) err={}, want {}",
            got, want
        );
    } else {
        assert!(false, "must be error");
    }

    Ok(())
}

#[test]
fn test_valid_compound() -> Result<(), Error> {
    let cname = Packet::SourceDescription(SourceDescription {
        chunks: vec![SourceDescriptionChunk {
            source: 1234,
            items: vec![SourceDescriptionItem {
                sdes_type: SDESType::SDESCNAME,
                text: "cname".to_string(),
            }],
        }],
    });

    let tests = vec![
        (
            "empty",
            CompoundPacket(vec![]),
            Some(ERR_EMPTY_COMPOUND.clone()),
        ),
        (
            "no cname",
            CompoundPacket(vec![Packet::SenderReport(SenderReport::default())]),
            Some(ERR_MISSING_CNAME.clone()),
        ),
        (
            "just BYE",
            CompoundPacket(vec![Packet::Goodbye(Goodbye::default())]),
            Some(ERR_BAD_FIRST_PACKET.clone()),
        ),
        (
            "SDES / no cname",
            CompoundPacket(vec![
                Packet::SenderReport(SenderReport::default()),
                Packet::SourceDescription(SourceDescription::default()),
            ]),
            Some(ERR_MISSING_CNAME.clone()),
        ),
        (
            "just SR",
            CompoundPacket(vec![
                Packet::SenderReport(SenderReport::default()),
                cname.clone(),
            ]),
            None,
        ),
        (
            "multiple SRs",
            CompoundPacket(vec![
                Packet::SenderReport(SenderReport::default()),
                Packet::SenderReport(SenderReport::default()),
                cname.clone(),
            ]),
            Some(ERR_PACKET_BEFORE_CNAME.clone()),
        ),
        (
            "just RR",
            CompoundPacket(vec![
                Packet::ReceiverReport(ReceiverReport::default()),
                cname.clone(),
            ]),
            None,
        ),
        (
            "multiple RRs",
            CompoundPacket(vec![
                Packet::ReceiverReport(ReceiverReport::default()),
                Packet::ReceiverReport(ReceiverReport::default()),
                cname.clone(),
            ]),
            None,
        ),
        (
            "goodbye",
            CompoundPacket(vec![
                Packet::ReceiverReport(ReceiverReport::default()),
                cname.clone(),
                Packet::Goodbye(Goodbye::default()),
            ]),
            None,
        ),
    ];

    for (name, compound_packet, want_error) in tests {
        let result = compound_packet.validate();
        if let Some(err) = want_error {
            if let Err(got) = result {
                assert_eq!(got, err, "validate {} : err = {}, want {}", name, got, err);
            } else {
                assert!(false, "want error in test {}", name);
            }
        } else {
            assert!(result.is_ok(), "must no error in test {}", name);
        }
    }

    Ok(())
}

#[test]
fn test_cname() -> Result<(), Error> {
    let cname = Packet::SourceDescription(SourceDescription {
        chunks: vec![SourceDescriptionChunk {
            source: 1234,
            items: vec![SourceDescriptionItem {
                sdes_type: SDESType::SDESCNAME,
                text: "cname".to_string(),
            }],
        }],
    });

    let tests = vec![
        (
            "no cname",
            CompoundPacket(vec![Packet::SenderReport(SenderReport::default())]),
            Some(ERR_MISSING_CNAME.clone()),
            "",
        ),
        (
            "SDES / no cname",
            CompoundPacket(vec![
                Packet::SenderReport(SenderReport::default()),
                Packet::SourceDescription(SourceDescription::default()),
            ]),
            Some(ERR_MISSING_CNAME.clone()),
            "",
        ),
        (
            "just SR",
            CompoundPacket(vec![
                Packet::SenderReport(SenderReport::default()),
                cname.clone(),
            ]),
            None,
            "cname",
        ),
        (
            "multiple SRs",
            CompoundPacket(vec![
                Packet::SenderReport(SenderReport::default()),
                Packet::SenderReport(SenderReport::default()),
                cname.clone(),
            ]),
            Some(ERR_PACKET_BEFORE_CNAME.clone()),
            "",
        ),
        (
            "just RR",
            CompoundPacket(vec![
                Packet::ReceiverReport(ReceiverReport::default()),
                cname.clone(),
            ]),
            None,
            "cname",
        ),
        (
            "multiple RRs",
            CompoundPacket(vec![
                Packet::ReceiverReport(ReceiverReport::default()),
                Packet::ReceiverReport(ReceiverReport::default()),
                cname.clone(),
            ]),
            None,
            "cname",
        ),
        (
            "goodbye",
            CompoundPacket(vec![
                Packet::ReceiverReport(ReceiverReport::default()),
                cname.clone(),
                Packet::Goodbye(Goodbye::default()),
            ]),
            None,
            "cname",
        ),
    ];

    for (name, compound_packet, want_error, text) in tests {
        let result = compound_packet.validate();
        if let Some(err) = want_error {
            if let Err(got) = result {
                assert_eq!(got, err, "validate {} : err = {}, want {}", name, got, err);
            } else {
                assert!(false, "want error in test {}", name);
            }
        } else {
            assert!(result.is_ok(), "must no error in test {}", name);
            if let Ok(cname) = compound_packet.cname() {
                assert_eq!(cname, text, "test {} = {}, want {}", name, cname, text);
            } else {
                assert!(false, "want cname in test {}", name);
            }
        }
    }

    Ok(())
}

#[test]
fn test_compound_packet_roundtrip() -> Result<(), Error> {
    let cname = Packet::SourceDescription(SourceDescription {
        chunks: vec![SourceDescriptionChunk {
            source: 1234,
            items: vec![SourceDescriptionItem {
                sdes_type: SDESType::SDESCNAME,
                text: "cname".to_string(),
            }],
        }],
    });

    let tests = vec![
        (
            "goodbye",
            CompoundPacket(vec![
                Packet::ReceiverReport(ReceiverReport::default()),
                cname.clone(),
                Packet::Goodbye(Goodbye::default()),
            ]),
            None,
        ),
        (
            "no cname",
            CompoundPacket(vec![Packet::ReceiverReport(ReceiverReport::default())]),
            Some(ERR_MISSING_CNAME.clone()),
        ),
    ];

    for (name, packet, marshal_error) in tests {
        let mut data: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(data.as_mut());
            let result = packet.marshal(&mut writer);
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
        }

        let _decoded = unmarshal(&data)?;

        let mut expect: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(expect.as_mut());
            let _result = packet.marshal(&mut writer);
        }
        assert_eq!(
            data, expect,
            "{} header round trip: got {:?}, want {:?}",
            name, data, expect
        )
    }

    Ok(())
}
