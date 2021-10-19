use super::*;

fn decoded_packet() -> ExtendedReport {
    ExtendedReport {
        sender_ssrc: 0x01020304,
        reports: vec![
            Box::new(LossRLEReportBlock {
                is_loss_rle: true,
                t: 12,

                ssrc: 0x12345689,
                begin_seq: 5,
                end_seq: 12,
                chunks: vec![Chunk(0x4006), Chunk(0x0006), Chunk(0x8765), Chunk(0x0000)],
            }),
            Box::new(DuplicateRLEReportBlock {
                is_loss_rle: false,
                t: 6,

                ssrc: 0x12345689,
                begin_seq: 5,
                end_seq: 12,
                chunks: vec![Chunk(0x4123), Chunk(0x3FFF), Chunk(0xFFFF), Chunk(0x0000)],
            }),
            Box::new(PacketReceiptTimesReportBlock {
                t: 3,

                ssrc: 0x98765432,
                begin_seq: 15432,
                end_seq: 15577,
                receipt_time: vec![0x11111111, 0x22222222, 0x33333333, 0x44444444, 0x55555555],
            }),
            Box::new(ReceiverReferenceTimeReportBlock {
                ntp_timestamp: 0x0102030405060708,
            }),
            Box::new(DLRRReportBlock {
                reports: vec![
                    DLRRReport {
                        ssrc: 0x88888888,
                        last_rr: 0x12345678,
                        dlrr: 0x99999999,
                    },
                    DLRRReport {
                        ssrc: 0x09090909,
                        last_rr: 0x12345678,
                        dlrr: 0x99999999,
                    },
                    DLRRReport {
                        ssrc: 0x11223344,
                        last_rr: 0x12345678,
                        dlrr: 0x99999999,
                    },
                ],
            }),
            Box::new(StatisticsSummaryReportBlock {
                loss_reports: true,
                duplicate_reports: true,
                jitter_reports: true,
                ttl_or_hop_limit: TTLorHopLimitType::IPv4,

                ssrc: 0xFEDCBA98,
                begin_seq: 0x1234,
                end_seq: 0x5678,
                lost_packets: 0x11111111,
                dup_packets: 0x22222222,
                min_jitter: 0x33333333,
                max_jitter: 0x44444444,
                mean_jitter: 0x55555555,
                dev_jitter: 0x66666666,
                min_ttl_or_hl: 0x01,
                max_ttl_or_hl: 0x02,
                mean_ttl_or_hl: 0x03,
                dev_ttl_or_hl: 0x04,
            }),
            Box::new(VoIPMetricsReportBlock {
                ssrc: 0x89ABCDEF,
                loss_rate: 0x05,
                discard_rate: 0x06,
                burst_density: 0x07,
                gap_density: 0x08,
                burst_duration: 0x1111,
                gap_duration: 0x2222,
                round_trip_delay: 0x3333,
                end_system_delay: 0x4444,
                signal_level: 0x11,
                noise_level: 0x22,
                rerl: 0x33,
                gmin: 0x44,
                rfactor: 0x55,
                ext_rfactor: 0x66,
                mos_lq: 0x77,
                mos_cq: 0x88,
                rx_config: 0x99,
                reserved: 0x00,
                jb_nominal: 0x1122,
                jb_maximum: 0x3344,
                jb_abs_max: 0x5566,
            }),
        ],
    }
}

fn encoded_packet() -> Bytes {
    Bytes::from_static(&[
        // RTP Header
        0x80, 0xCF, 0x00, 0x33, // byte 0 - 3
        // Sender SSRC
        0x01, 0x02, 0x03, 0x04, // Loss RLE Report Block
        0x01, 0x0C, 0x00, 0x04, // byte 8 - 11
        // Source SSRC
        0x12, 0x34, 0x56, 0x89, // Begin & End Seq
        0x00, 0x05, 0x00, 0x0C, // byte 16 - 19
        // Chunks
        0x40, 0x06, 0x00, 0x06, 0x87, 0x65, 0x00, 0x00, // byte 24 - 27
        // Duplicate RLE Report Block
        0x02, 0x06, 0x00, 0x04, // Source SSRC
        0x12, 0x34, 0x56, 0x89, // byte 32 - 35
        // Begin & End Seq
        0x00, 0x05, 0x00, 0x0C, // Chunks
        0x41, 0x23, 0x3F, 0xFF, // byte 40 - 43
        0xFF, 0xFF, 0x00, 0x00, // Packet Receipt Times Report Block
        0x03, 0x03, 0x00, 0x07, // byte 48 - 51
        // Source SSRC
        0x98, 0x76, 0x54, 0x32, // Begin & End Seq
        0x3C, 0x48, 0x3C, 0xD9, // byte 56 - 59
        // Receipt times
        0x11, 0x11, 0x11, 0x11, 0x22, 0x22, 0x22, 0x22, // byte 64 - 67
        0x33, 0x33, 0x33, 0x33, 0x44, 0x44, 0x44, 0x44, // byte 72 - 75
        0x55, 0x55, 0x55, 0x55, // Receiver Reference Time Report
        0x04, 0x00, 0x00, 0x02, // byte 80 - 83
        // Timestamp
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // byte 88 - 91
        // DLRR Report
        0x05, 0x00, 0x00, 0x09, // SSRC 1
        0x88, 0x88, 0x88, 0x88, // byte 96 - 99
        // LastRR 1
        0x12, 0x34, 0x56, 0x78, // DLRR 1
        0x99, 0x99, 0x99, 0x99, // byte 104 - 107
        // SSRC 2
        0x09, 0x09, 0x09, 0x09, // LastRR 2
        0x12, 0x34, 0x56, 0x78, // byte 112 - 115
        // DLRR 2
        0x99, 0x99, 0x99, 0x99, // SSRC 3
        0x11, 0x22, 0x33, 0x44, // byte 120 - 123
        // LastRR 3
        0x12, 0x34, 0x56, 0x78, // DLRR 3
        0x99, 0x99, 0x99, 0x99, // byte 128 - 131
        // Statistics Summary Report
        0x06, 0xE8, 0x00, 0x09, // SSRC
        0xFE, 0xDC, 0xBA, 0x98, // byte 136 - 139
        // Various statistics
        0x12, 0x34, 0x56, 0x78, 0x11, 0x11, 0x11, 0x11, // byte 144 - 147
        0x22, 0x22, 0x22, 0x22, 0x33, 0x33, 0x33, 0x33, // byte 152 - 155
        0x44, 0x44, 0x44, 0x44, 0x55, 0x55, 0x55, 0x55, // byte 160 - 163
        0x66, 0x66, 0x66, 0x66, 0x01, 0x02, 0x03, 0x04, // byte 168 - 171
        // VoIP Metrics Report
        0x07, 0x00, 0x00, 0x08, // SSRC
        0x89, 0xAB, 0xCD, 0xEF, // byte 176 - 179
        // Various statistics
        0x05, 0x06, 0x07, 0x08, 0x11, 0x11, 0x22, 0x22, // byte 184 - 187
        0x33, 0x33, 0x44, 0x44, 0x11, 0x22, 0x33, 0x44, // byte 192 - 195
        0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0x11, 0x22, // byte 200 - 203
        0x33, 0x44, 0x55, 0x66, // byte 204 - 207
    ])
}

#[test]
fn test_encode() -> Result<()> {
    let expected = encoded_packet();
    let packet = decoded_packet();
    let actual = packet.marshal()?;
    assert_eq!(actual, expected);
    Ok(())
}

#[test]
fn test_decode() -> Result<()> {
    let mut encoded = encoded_packet();
    let expected = decoded_packet();
    let actual = ExtendedReport::unmarshal(&mut encoded)?;
    assert_eq!(actual, expected);
    assert_eq!(actual.to_string(), expected.to_string());
    Ok(())
}
