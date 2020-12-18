#[cfg(test)]
mod test {

    use crate::{
        goodbye, packet::*, picture_loss_indication, rapid_resynchronization_request,
        reception_report, source_description,
    };

    const BYTES: [u8; 116] = [
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
        0x0, 0x0, 0x0, 0x0, // Goodbye (offset=84)
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

        assert_eq!(packet.len(), 5);

        match packet[0]
            .as_any()
            .downcast_ref::<receiver_report::ReceiverReport>()
        {
            Some(val) => {
                let val: &receiver_report::ReceiverReport = val;

                assert_eq!(*val, a, "Error comparing receiver report results");
            }

            None => panic!("Trait panic on downcasting to receiver_report."),
        }

        match packet[1]
            .as_any()
            .downcast_ref::<source_description::SourceDescription>()
        {
            Some(val) => {
                let val: &source_description::SourceDescription = val;

                assert_eq!(*val, b, "Error comparing receiver report results");
            }

            None => panic!("Trait panic on downcasting to receiver_report."),
        }

        match packet[2].as_any().downcast_ref::<goodbye::Goodbye>() {
            Some(val) => {
                let val: &goodbye::Goodbye = val;

                assert_eq!(*val, c, "Error comparing receiver report results");
            }

            None => panic!("Trait panic on downcasting to receiver_report."),
        }

        match packet[3]
            .as_any()
            .downcast_ref::<picture_loss_indication::PictureLossIndication>()
        {
            Some(val) => {
                let val: &picture_loss_indication::PictureLossIndication = val;

                assert_eq!(*val, d, "Error comparing receiver report results");
            }

            None => panic!("Trait panic on downcasting to receiver_report."),
        }

        match packet[4]
            .as_any()
            .downcast_ref::<rapid_resynchronization_request::RapidResynchronizationRequest>()
        {
            Some(val) => {
                let val: &rapid_resynchronization_request::RapidResynchronizationRequest = val;

                assert_eq!(*val, e, "Error comparing receiver report results");
            }

            None => panic!("Trait panic on downcasting to receiver_report."),
        }
    }

    #[test]
    fn test_packet_unmarshal_empty() -> Result<(), Error> {
        let result = unmarshal(BytesMut::new());
        if let Err(got) = result {
            let want = ERR_INVALID_HEADER.clone();
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
            let want = ERR_PACKET_TOO_SHORT.clone();
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
