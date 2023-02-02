use rtp::{header::Header, packet::Packet, packetizer::Depacketizer};

use super::*;

// Turns u8 integers into Bytes Array
macro_rules! bytes {
    ($($item:expr),*) => ({
        static STATIC_SLICE: &'static [u8] = &[$($item), *];
        Bytes::from_static(STATIC_SLICE)
    });
}
#[derive(Default)]
pub struct SampleBuilderTest {
    message: String,
    packets: Vec<Packet>,
    with_head_checker: bool,
    head_bytes: Vec<bytes::Bytes>,
    samples: Vec<Sample>,
    max_late: u16,
    max_late_timestamp: Duration,
    extra_pop_attempts: usize,
}

pub struct FakeDepacketizer {
    head_checker: bool,
    head_bytes: Vec<bytes::Bytes>,
}

impl FakeDepacketizer {
    fn new() -> Self {
        Self {
            head_checker: false,
            head_bytes: vec![],
        }
    }
}

impl Depacketizer for FakeDepacketizer {
    fn depacketize(&mut self, b: &Bytes) -> std::result::Result<bytes::Bytes, rtp::Error> {
        Ok(b.clone())
    }

    /// Checks if the packet is at the beginning of a partition.  This
    /// should return false if the result could not be determined, in
    /// which case the caller will detect timestamp discontinuities.
    fn is_partition_head(&self, payload: &Bytes) -> bool {
        if !self.head_checker {
            // from .go: simulates a bug in 3.0 version, the tests should not assume the bug
            return true;
        }

        for b in &self.head_bytes {
            if *payload == b {
                return true;
            }
        }
        false
    }

    /// Checks if the packet is at the end of a partition.  This should
    /// return false if the result could not be determined.
    fn is_partition_tail(&self, marker: bool, _payload: &Bytes) -> bool {
        marker
    }
}

#[test]
pub fn test_sample_builder() {
    #![allow(clippy::needless_update)]
    let test_data: Vec<SampleBuilderTest> = vec![
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder shouldn't emit anything if only one RTP packet has been pushed".into(),
            packets: vec![Packet {
                header: Header {
                    sequence_number: 5000,
                    timestamp: 5,
                    ..Default::default()
                },
                payload: bytes!(1),
                ..Default::default()
            }],
            samples: vec![],
            max_late: 50,
            max_late_timestamp: Duration::from_secs(0),
            ..Default::default()
        },
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder shouldn't emit anything if only one RTP packet has been pushed even if the marker bit is set".into(),
            packets: vec![Packet {
                header: Header {
                    sequence_number: 5000,
                    timestamp: 5,
                    marker: true,
                    ..Default::default()
                },
                payload: bytes!(1),
                ..Default::default()
            }],
            samples: vec![],
            max_late: 50,
            max_late_timestamp: Duration::from_secs(0),
            ..Default::default()
        },
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder should emit two packets, we had three packets with unique timestamps".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 5,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5001,
                        timestamp: 6,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5002,
                        timestamp: 7,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
            ],
            samples: vec![
                Sample {
                    // First sample
                    data: bytes!(1),
                    duration: Duration::from_secs(1), // technically this is the default value, but since it was in .go source....
                    packet_timestamp: 5,
                    ..Default::default()
                },
                Sample {
                    // Second sample
                    data: bytes!(2),
                    duration: Duration::from_secs(1),
                    packet_timestamp: 6,
                    ..Default::default()
                },
            ],
            max_late: 50,
            max_late_timestamp: Duration::from_secs(0),
            ..Default::default()
        },
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder should emit one packet, we had a packet end of sequence marker and run out of space".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 5,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5002,
                        timestamp: 7,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5004,
                        timestamp: 9,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
                Packet {
                    // Fourth packet
                    header: Header {
                        sequence_number: 5006,
                        timestamp: 11,
                        ..Default::default()
                    },
                    payload: bytes!(4),
                    ..Default::default()
                },
                Packet {
                    // Fifth packet
                    header: Header {
                        sequence_number: 5008,
                        timestamp: 13,
                        ..Default::default()
                    },
                    payload: bytes!(5),
                    ..Default::default()
                },
                Packet {
                    // Sixth packet
                    header: Header {
                        sequence_number: 5010,
                        timestamp: 15,
                        ..Default::default()
                    },
                    payload: bytes!(6),
                    ..Default::default()
                },
                Packet {
                    // Seventh packet
                    header: Header {
                        sequence_number: 5012,
                        timestamp: 17,
                        ..Default::default()
                    },
                    payload: bytes!(7),
                    ..Default::default()
                },
            ],
            samples: vec![Sample {
                // First sample
                data: bytes!(1),
                duration: Duration::from_secs(2),
                packet_timestamp: 5,
                ..Default::default()
            }],
            max_late: 5,
            max_late_timestamp: Duration::from_secs(0),
            ..Default::default()
        },
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder shouldn't emit any packet, we do not have a valid end of sequence and run out of space".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 5,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5002,
                        timestamp: 7,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5004,
                        timestamp: 9,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
                Packet {
                    // Fourth packet
                    header: Header {
                        sequence_number: 5006,
                        timestamp: 11,
                        ..Default::default()
                    },
                    payload: bytes!(4),
                    ..Default::default()
                },
                Packet {
                    // Fifth packet
                    header: Header {
                        sequence_number: 5008,
                        timestamp: 13,
                        ..Default::default()
                    },
                    payload: bytes!(5),
                    ..Default::default()
                },
                Packet {
                    // Sixth packet
                    header: Header {
                        sequence_number: 5010,
                        timestamp: 15,
                        ..Default::default()
                    },
                    payload: bytes!(6),
                    ..Default::default()
                },
                Packet {
                    // Seventh packet
                    header: Header {
                        sequence_number: 5012,
                        timestamp: 17,
                        ..Default::default()
                    },
                    payload: bytes!(7),
                    ..Default::default()
                },
            ],
            samples: vec![],
            max_late: 5,
            max_late_timestamp: Duration::from_secs(0),
            ..Default::default()
        },
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder should emit one packet, we had a packet end of sequence marker and run out of space".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 5,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5002,
                        timestamp: 7,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5004,
                        timestamp: 9,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
                Packet {
                    // Fourth packet
                    header: Header {
                        sequence_number: 5006,
                        timestamp: 11,
                        ..Default::default()
                    },
                    payload: bytes!(4),
                    ..Default::default()
                },
                Packet {
                    // Fifth packet
                    header: Header {
                        sequence_number: 5008,
                        timestamp: 13,
                        ..Default::default()
                    },
                    payload: bytes!(5),
                    ..Default::default()
                },
                Packet {
                    // Sixth packet
                    header: Header {
                        sequence_number: 5010,
                        timestamp: 15,
                        ..Default::default()
                    },
                    payload: bytes!(6),
                    ..Default::default()
                },
                Packet {
                    // Seventh packet
                    header: Header {
                        sequence_number: 5012,
                        timestamp: 17,
                        ..Default::default()
                    },
                    payload: bytes!(7),
                    ..Default::default()
                },
            ],
            samples: vec![
                Sample {
                    // First (dropped) sample
                    data: bytes!(1),
                    duration: Duration::from_secs(2),
                    packet_timestamp: 5,
                    ..Default::default()
                },
                Sample {
                    // First correct sample
                    data: bytes!(2),
                    duration: Duration::from_secs(2),
                    packet_timestamp: 7,
                    prev_dropped_packets: 1,
                    ..Default::default()
                },
            ],
            max_late: 5,
            max_late_timestamp: Duration::from_secs(0),
            ..Default::default()
        },
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder should emit one packet, we had two packets but with duplicate timestamps".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 5,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5001,
                        timestamp: 6,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5002,
                        timestamp: 6,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
                Packet {
                    // Fourth packet
                    header: Header {
                        sequence_number: 5003,
                        timestamp: 7,
                        ..Default::default()
                    },
                    payload: bytes!(4),
                    ..Default::default()
                },
            ],
            samples: vec![
                Sample {
                    // First sample
                    data: bytes!(1),
                    duration: Duration::from_secs(1),
                    packet_timestamp: 5,
                    ..Default::default()
                },
                Sample {
                    // Second (duplicate) correct sample
                    data: bytes!(2, 3),
                    duration: Duration::from_secs(1),
                    packet_timestamp: 6,
                    ..Default::default()
                },
            ],
            max_late: 50,
            max_late_timestamp: Duration::from_secs(0),
            ..Default::default()
        },
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder shouldn't emit a packet because we have a gap before a valid one".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 5,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5007,
                        timestamp: 6,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5008,
                        timestamp: 7,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
            ],
            samples: vec![],
            max_late: 50,
            max_late_timestamp: Duration::from_secs(0),
            ..Default::default()
        },
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder shouldn't emit a packet after a gap as there are gaps and have not reached maxLate yet".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 5,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5007,
                        timestamp: 6,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5008,
                        timestamp: 7,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
            ],
            with_head_checker: true,
            head_bytes: vec![bytes!(2)],
            samples: vec![],
            max_late: 50,
            max_late_timestamp: Duration::from_secs(0),
            ..Default::default()
        },
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder shouldn't emit a packet after a gap if PartitionHeadChecker doesn't assume it head".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 5,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5007,
                        timestamp: 6,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5008,
                        timestamp: 7,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
            ],
            with_head_checker: true,
            head_bytes: vec![],
            samples: vec![],
            max_late: 50,
            max_late_timestamp: Duration::from_secs(0),
            ..Default::default()
        },
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder should emit multiple valid packets".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5001,
                        timestamp: 2,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5002,
                        timestamp: 3,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
                Packet {
                    // Fourth packet
                    header: Header {
                        sequence_number: 5003,
                        timestamp: 4,
                        ..Default::default()
                    },
                    payload: bytes!(4),
                    ..Default::default()
                },
                Packet {
                    // Fifth packet
                    header: Header {
                        sequence_number: 5004,
                        timestamp: 5,
                        ..Default::default()
                    },
                    payload: bytes!(5),
                    ..Default::default()
                },
                Packet {
                    // Sixth packet
                    header: Header {
                        sequence_number: 5005,
                        timestamp: 6,
                        ..Default::default()
                    },
                    payload: bytes!(6),
                    ..Default::default()
                },
            ],
            samples: vec![
                Sample {
                    // First sample
                    data: bytes!(1),
                    duration: Duration::from_secs(1),
                    packet_timestamp: 1,
                    ..Default::default()
                },
                Sample {
                    // Second sample
                    data: bytes!(2),
                    duration: Duration::from_secs(1),
                    packet_timestamp: 2,
                    ..Default::default()
                },
                Sample {
                    // Third sample
                    data: bytes!(3),
                    duration: Duration::from_secs(1),
                    packet_timestamp: 3,
                    ..Default::default()
                },
                Sample {
                    // Fourth sample
                    data: bytes!(4),
                    duration: Duration::from_secs(1),
                    packet_timestamp: 4,
                    ..Default::default()
                },
                Sample {
                    // Fifth sample
                    data: bytes!(5),
                    duration: Duration::from_secs(1),
                    packet_timestamp: 5,
                    ..Default::default()
                },
            ],
            max_late: 50,
            max_late_timestamp: Duration::from_secs(0),
            ..Default::default()
        },
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder should skip timestamps too old".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5001,
                        timestamp: 2,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5002,
                        timestamp: 3,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
                Packet {
                    // Fourth packet
                    header: Header {
                        sequence_number: 5013,
                        timestamp: 4000,
                        ..Default::default()
                    },
                    payload: bytes!(4),
                    ..Default::default()
                },
                Packet {
                    // Fifth packet
                    header: Header {
                        sequence_number: 5014,
                        timestamp: 4000,
                        ..Default::default()
                    },
                    payload: bytes!(5),
                    ..Default::default()
                },
                Packet {
                    // Sixth packet
                    header: Header {
                        sequence_number: 5015,
                        timestamp: 4002,
                        ..Default::default()
                    },
                    payload: bytes!(6),
                    ..Default::default()
                },
                Packet {
                    // Seventh packet
                    header: Header {
                        sequence_number: 5016,
                        timestamp: 7000,
                        ..Default::default()
                    },
                    payload: bytes!(4),
                    ..Default::default()
                },
                Packet {
                    // Eigth packet
                    header: Header {
                        sequence_number: 5017,
                        timestamp: 7001,
                        ..Default::default()
                    },
                    payload: bytes!(5),
                    ..Default::default()
                },
            ],
            samples: vec![Sample {
                // First sample
                data: bytes!(4, 5),
                duration: Duration::from_secs(2),
                packet_timestamp: 4000,
                prev_dropped_packets: 12,
                ..Default::default()
            }],
            with_head_checker: true,
            head_bytes: vec![bytes!(4)],
            max_late: 50,
            max_late_timestamp: Duration::from_secs(2000),
            ..Default::default()
        },
        // This test is based on observed RTP packet streams from Chrome. libWebRTC inserts padding
        // packets to keep send rates steady, these are not important for sample building but we
        // should identify them as padding packets to differentiate them from lost packets.
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder should recognise padding packets".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5001,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5002,
                        timestamp: 1,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
                Packet {
                    // Padding packet 1
                    header: Header {
                        sequence_number: 5003,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: Bytes::from_static(&[]),
                    ..Default::default()
                },
                Packet {
                    // Padding packet 2
                    header: Header {
                        sequence_number: 5004,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: Bytes::from_static(&[]),
                    ..Default::default()
                },
                Packet {
                    // Sixth packet
                    header: Header {
                        sequence_number: 5005,
                        timestamp: 2,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Seventh packet
                    header: Header {
                        sequence_number: 5006,
                        timestamp: 2,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(7),
                    ..Default::default()
                },
                Packet {
                    // Seventh packet
                    header: Header {
                        sequence_number: 5007,
                        timestamp: 3,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
            ],
            samples: vec![
                Sample {
                    // First sample
                    data: bytes!(1, 2, 3),
                    duration: Duration::from_secs(0),
                    packet_timestamp: 1,
                    prev_dropped_packets: 0,
                    ..Default::default()
                },
                Sample {
                    // Second sample
                    data: bytes!(1, 7),
                    duration: Duration::from_secs(1),
                    packet_timestamp: 2,
                    prev_dropped_packets: 2,
                    prev_padding_packets: 2,
                    ..Default::default()
                },
            ],
            with_head_checker: true,
            head_bytes: vec![bytes!(1)],
            max_late: 50,
            max_late_timestamp: Duration::from_secs(2000),
            extra_pop_attempts: 1,
            ..Default::default()
        },
        // This test is based on observed RTP packet streams when screen sharing in Chrome.
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder should recognise padding packets when combined with max_late_timestamp".into(),
            packets: vec![
                Packet {
                    // First packet
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Second packet
                    header: Header {
                        sequence_number: 5001,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                Packet {
                    // Third packet
                    header: Header {
                        sequence_number: 5002,
                        timestamp: 1,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(3),
                    ..Default::default()
                },
                Packet {
                    // Padding packet 1
                    header: Header {
                        sequence_number: 5003,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: Bytes::from_static(&[]),
                    ..Default::default()
                },
                Packet {
                    // Padding packet 2
                    header: Header {
                        sequence_number: 5004,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: Bytes::from_static(&[]),
                    ..Default::default()
                },
                Packet {
                    // Sixth packet
                    header: Header {
                        sequence_number: 5005,
                        timestamp: 3,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    // Seventh packet
                    header: Header {
                        sequence_number: 5006,
                        timestamp: 3,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(7),
                    ..Default::default()
                },
                Packet {
                    // Seventh packet
                    header: Header {
                        sequence_number: 5007,
                        timestamp: 4,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
            ],
            samples: vec![
                Sample {
                    // First sample
                    data: bytes!(1, 2, 3),
                    duration: Duration::from_secs(0),
                    packet_timestamp: 1,
                    prev_dropped_packets: 0,
                    ..Default::default()
                },
                Sample {
                    // Second sample
                    data: bytes!(1, 7),
                    duration: Duration::from_secs(1),
                    packet_timestamp: 3,
                    prev_dropped_packets: 2,
                    prev_padding_packets: 2,
                    ..Default::default()
                },
            ],
            with_head_checker: true,
            head_bytes: vec![bytes!(1)],
            max_late: 50,
            max_late_timestamp: Duration::from_millis(1050),
            extra_pop_attempts: 1,
            ..Default::default()
        },
        // This test is based on observed RTP packet streams when screen sharing in Chrome.
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder should build a sample out of a packet that's both start and end".into(),
            packets: vec![
                Packet {
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 1,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                Packet {
                    header: Header {
                        sequence_number: 5001,
                        timestamp: 2,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
            ],
            samples: vec![Sample {
                // First sample
                data: bytes!(1),
                duration: Duration::from_secs(1),
                packet_timestamp: 1,
                prev_dropped_packets: 0,
                ..Default::default()
            }],
            with_head_checker: true,
            head_bytes: vec![bytes!(1)],
            max_late: 50,
            max_late_timestamp: Duration::from_millis(1050),
            ..Default::default()
        },
        // This test is based on observed RTP packet streams when screen sharing in Chrome. In
        // particular the scenario used involved no movement on screen which causes Chrome to
        // generate padding packets.
        SampleBuilderTest {
            #[rustfmt::skip]
            message: "Sample builder should build a sample out of a packet that's both start and end following a run of padding packets".into(),
            packets: vec![
                // First valid packet
                Packet {
                    header: Header {
                        sequence_number: 5000,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                // Second valid packet
                Packet {
                    header: Header {
                        sequence_number: 5001,
                        timestamp: 1,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(2),
                    ..Default::default()
                },
                // Padding packet 1
                Packet {
                    header: Header {
                        sequence_number: 5002,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: Bytes::default(),
                    ..Default::default()
                },
                // Padding packet 2
                Packet {
                    header: Header {
                        sequence_number: 5003,
                        timestamp: 1,
                        ..Default::default()
                    },
                    payload: Bytes::default(),
                    ..Default::default()
                },
                // Third valid packet
                Packet {
                    header: Header {
                        sequence_number: 5004,
                        timestamp: 2,
                        marker: true,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
                // Fourth valid packet, start of next sample
                Packet {
                    header: Header {
                        sequence_number: 5005,
                        timestamp: 3,
                        ..Default::default()
                    },
                    payload: bytes!(1),
                    ..Default::default()
                },
            ],
            samples: vec![
                Sample {
                    // First sample
                    data: bytes!(1, 2),
                    duration: Duration::from_secs(0),
                    packet_timestamp: 1,
                    prev_dropped_packets: 0,
                    ..Default::default()
                },
                Sample {
                    // Second sample
                    data: bytes!(1),
                    duration: Duration::from_secs(1),
                    packet_timestamp: 2,
                    prev_dropped_packets: 2,
                    prev_padding_packets: 2,
                    ..Default::default()
                },
            ],
            with_head_checker: true,
            head_bytes: vec![bytes!(1)],
            extra_pop_attempts: 1,
            max_late: 50,
            ..Default::default()
        },
    ];

    for t in test_data {
        let d = FakeDepacketizer {
            head_checker: t.with_head_checker,
            head_bytes: t.head_bytes,
        };

        let mut s = {
            let sample_builder = SampleBuilder::new(t.max_late, d, 1);
            if t.max_late_timestamp != Duration::from_secs(0) {
                sample_builder.with_max_time_delay(t.max_late_timestamp)
            } else {
                sample_builder
            }
        };

        let mut samples = Vec::<Sample>::new();
        for p in t.packets {
            s.push(p)
        }

        while let Some(sample) = s.pop() {
            samples.push(sample)
        }

        for _ in 0..t.extra_pop_attempts {
            // Pop some more
            while let Some(sample) = s.pop() {
                samples.push(sample)
            }
        }

        // Current problem: Sample does not implement Eq. Either implement myself or find another way of comparison. (Derive does not work)
        assert_eq!(t.samples, samples, "{}", t.message);
    }
}

// SampleBuilder should respect maxLate if we popped successfully but then have a gap larger then maxLate
#[test]
fn test_sample_builder_max_late() {
    let mut s = SampleBuilder::new(50, FakeDepacketizer::new(), 1);

    s.push(Packet {
        header: Header {
            sequence_number: 0,
            timestamp: 1,
            ..Default::default()
        },
        payload: bytes!(0x01),
    });
    s.push(Packet {
        header: Header {
            sequence_number: 1,
            timestamp: 2,
            ..Default::default()
        },
        payload: bytes!(0x01),
    });
    s.push(Packet {
        header: Header {
            sequence_number: 2,
            timestamp: 3,
            ..Default::default()
        },
        payload: bytes!(0x01),
    });
    assert_eq!(
        s.pop(),
        Some(Sample {
            data: bytes!(0x01),
            duration: Duration::from_secs(1),
            packet_timestamp: 1,
            ..Default::default()
        }),
        "Failed to build samples before gap"
    );

    s.push(Packet {
        header: Header {
            sequence_number: 5000,
            timestamp: 500,
            ..Default::default()
        },
        payload: bytes!(0x02),
    });
    s.push(Packet {
        header: Header {
            sequence_number: 5001,
            timestamp: 501,
            ..Default::default()
        },
        payload: bytes!(0x02),
    });
    s.push(Packet {
        header: Header {
            sequence_number: 5002,
            timestamp: 502,
            ..Default::default()
        },
        payload: bytes!(0x02),
    });

    assert_eq!(
        s.pop(),
        Some(Sample {
            data: bytes!(0x01),
            duration: Duration::from_secs(1),
            packet_timestamp: 2,
            ..Default::default()
        }),
        "Failed to build samples after large gap"
    );
    assert_eq!(None, s.pop(), "Failed to build samples after large gap");

    s.push(Packet {
        header: Header {
            sequence_number: 6000,
            timestamp: 600,
            ..Default::default()
        },
        payload: bytes!(0x03),
    });
    assert_eq!(
        s.pop(),
        Some(Sample {
            data: bytes!(0x02),
            duration: Duration::from_secs(1),
            packet_timestamp: 500,
            prev_dropped_packets: 4998,
            ..Default::default()
        }),
        "Failed to build samples after large gap"
    );
    assert_eq!(
        s.pop(),
        Some(Sample {
            data: bytes!(0x02),
            duration: Duration::from_secs(1),
            packet_timestamp: 501,
            ..Default::default()
        }),
        "Failed to build samples after large gap"
    );
}

#[test]
fn test_seqnum_distance() {
    struct TestData {
        x: u16,
        y: u16,
        d: u16,
    }
    let test_data = vec![
        TestData {
            x: 0x0001,
            y: 0x0003,
            d: 0x0002,
        },
        TestData {
            x: 0x0003,
            y: 0x0001,
            d: 0x0002,
        },
        TestData {
            x: 0xFFF3,
            y: 0xFFF1,
            d: 0x0002,
        },
        TestData {
            x: 0xFFF1,
            y: 0xFFF3,
            d: 0x0002,
        },
        TestData {
            x: 0xFFFF,
            y: 0x0001,
            d: 0x0002,
        },
        TestData {
            x: 0x0001,
            y: 0xFFFF,
            d: 0x0002,
        },
    ];

    for data in test_data {
        assert_eq!(
            seqnum_distance(data.x, data.y),
            data.d,
            "seqnum_distance({}, {}) returned {} which must be {}",
            data.x,
            data.y,
            seqnum_distance(data.x, data.y),
            data.d
        );
    }
}

#[test]
fn test_sample_builder_clean_reference() {
    for seq_start in [0_u16, 0xfff8, 0xfffe] {
        let mut s = SampleBuilder::new(10, FakeDepacketizer::new(), 1);
        s.push(Packet {
            header: Header {
                sequence_number: seq_start,
                timestamp: 0,
                ..Default::default()
            },
            payload: bytes!(0x01),
        });
        s.push(Packet {
            header: Header {
                sequence_number: seq_start.wrapping_add(1),
                timestamp: 0,
                ..Default::default()
            },
            payload: bytes!(0x02),
        });
        s.push(Packet {
            header: Header {
                sequence_number: seq_start.wrapping_add(2),
                timestamp: 0,
                ..Default::default()
            },
            payload: bytes!(0x03),
        });
        let pkt4 = Packet {
            header: Header {
                sequence_number: seq_start.wrapping_add(14),
                timestamp: 120,
                ..Default::default()
            },
            payload: bytes!(0x04),
        };
        s.push(pkt4.clone());
        let pkt5 = Packet {
            header: Header {
                sequence_number: seq_start.wrapping_add(12),
                timestamp: 120,
                ..Default::default()
            },
            payload: bytes!(0x05),
        };
        s.push(pkt5.clone());

        for i in 0..3 {
            assert_eq!(
                s.buffer[seq_start.wrapping_add(i) as usize],
                None,
                "Old packet ({i}) is not unreferenced (seq_start: {seq_start}, max_late: 10, pushed: 12)"
            );
        }
        assert_eq!(s.buffer[seq_start.wrapping_add(14) as usize], Some(pkt4));
        assert_eq!(s.buffer[seq_start.wrapping_add(12) as usize], Some(pkt5));
    }
}

#[test]
fn test_sample_builder_push_max_zero() {
    let pkts = vec![Packet {
        header: Header {
            sequence_number: 0,
            timestamp: 0,
            marker: true,
            ..Default::default()
        },
        payload: bytes!(0x01),
    }];
    let d = FakeDepacketizer {
        head_checker: true,
        head_bytes: vec![bytes!(0x01)],
    };
    let mut s = SampleBuilder::new(0, d, 1);
    s.push(pkts[0].clone());
    assert!(s.pop().is_some(), "Should expect a popped sample.")
}

#[test]
fn test_pop_with_timestamp() {
    let mut s = SampleBuilder::new(0, FakeDepacketizer::new(), 1);
    assert_eq!(s.pop_with_timestamp(), None);
}

#[test]
fn test_sample_builder_data() {
    let mut s = SampleBuilder::new(10, FakeDepacketizer::new(), 1);
    let mut j: usize = 0;
    for i in 0..0x20000_usize {
        let p = Packet {
            header: Header {
                sequence_number: i as u16,
                timestamp: (i + 42) as u32,
                ..Default::default()
            },
            payload: Bytes::copy_from_slice(&[i as u8]),
        };
        s.push(p);
        while let Some((sample, ts)) = s.pop_with_timestamp() {
            assert_eq!(ts, (j + 42) as u32, "timestamp");
            assert_eq!(sample.data.len(), 1, "data length");
            assert_eq!(sample.data[0], j as u8, "timestamp");
            j += 1;
        }
    }
    // only the last packet should be dropped
    assert_eq!(j, 0x1FFFF);
}
