#[cfg(test)]
mod test {
    use crate::{
        error::Error, goodbye, packet::*, picture_loss_indication,
        rapid_resynchronization_request, reception_report, source_description,
    };

    const BYTES: [u8; 116] = [
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
    fn test_packet_unmarshal() {
        let packet = unmarshal(BYTES[..].into()).expect("Error unmarshalling packets");

        let a = receiver_report::ReceiverReport {
            ssrc: 0x902f9e2e,
            reports: vec![reception_report::ReceptionReport {
                ssrc: 0xbc5e9a40,
                fraction_lost: 0,
                total_lost: 0,
                last_sequence_number: 0x46e1,
                jitter: 273,
                last_sender_report: 0x9f36432,
                delay: 150137,
            }],
            ..Default::default()
        };

        let b = source_description::SourceDescription {
            chunks: vec![source_description::SourceDescriptionChunk {
                source: 0x902f9e2e,
                items: vec![source_description::SourceDescriptionItem {
                    sdes_type: source_description::SDESType::SDESCNAME,
                    text: "{9c00eb92-1afb-9d49-a47d-91f64eee69f5}".to_string(),
                }],
            }],
        };

        let c = goodbye::Goodbye {
            sources: vec![0x902f9e2e],
            ..Default::default()
        };

        let d = picture_loss_indication::PictureLossIndication {
            sender_ssrc: 0x902f9e2e,
            media_ssrc: 0x902f9e2e,
        };

        let e = rapid_resynchronization_request::RapidResynchronizationRequest {
            sender_ssrc: 0x902f9e2e,
            media_ssrc: 0x902f9e2e,
        };

        let expected: Vec<Box<dyn Packet>> = vec![
            Box::new(a),
            Box::new(b),
            Box::new(c),
            Box::new(d),
            Box::new(e),
        ];

        assert_eq!(packet.len(), 5);

        if packet != expected {
            panic!("Invalid packets")
        }
    }

    #[test]
    fn test_packet_unmarshal_empty() -> Result<(), Error> {
        let result = unmarshal(BytesMut::new());
        if let Err(got) = result {
            let want = Error::InvalidHeader;
            assert_eq!(got, want, "Unmarshal(nil) err = {}, want {}", got, want);
        } else {
            assert!(false, "want error");
        }

        Ok(())
    }

    #[test]
    fn test_packet_invalid_header_length() -> Result<(), Error> {
        let data: [u8; 4] = [
            // Goodbye (offset=84)
            // v=2, p=0, count=1, BYE, len=100
            0x81, 0xcb, 0x0, 0x64,
        ];

        let result = unmarshal(data[..].into());
        if let Err(got) = result {
            let want = Error::PacketTooShort;
            assert_eq!(
                got, want,
                "Unmarshal(invalid_header_length) err = {}, want {}",
                got, want
            );
        } else {
            assert!(false, "want error");
        }

        Ok(())
    }
}
