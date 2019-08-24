use super::*;
use crate::codecs::*;
use crate::sequence::*;

use std::io::BufReader;
use std::time::Duration;

use utils::Error;

use chrono::prelude::*;

#[test]
fn test_unix2ntp() -> Result<(), Error> {
    let loc = FixedOffset::west(5 * 60 * 60); // UTC-5
    let tests = vec![
        (
            loc.ymd(1985, 6, 23).and_hms_nano(4, 0, 0, 0),
            0xa0c65b1000000000 as u64,
        ),
        (
            loc.ymd(1999, 12, 31).and_hms_nano(23, 59, 59, 500000),
            0xbc18084f0020c49b as u64,
        ),
        (
            loc.ymd(2019, 3, 27).and_hms_nano(13, 39, 30, 8675309),
            0xe04641e202388b88 as u64,
        ),
    ];

    for (t, n) in tests {
        let ntp = unix2ntp(Duration::from_nanos(t.timestamp_nanos() as u64));
        assert_eq!(ntp, n, "unix2ntp error");
    }

    Ok(())
}

#[test]
fn test_packetizer() -> Result<(), Error> {
    let multiplepayload = vec![0; 128];
    let mut reader = BufReader::new(multiplepayload.as_slice());

    //use the G722 payloader here, because it's very simple and all 0s is valid G722 data.
    let mut packetizer = PacketizerImpl::new(100, 98, 0x1234ABCD, 90000);

    let mut g722 = g722::G722::default();
    let mut seq = new_random_sequencer();
    let packets = packetizer.packetize(&mut reader, &mut g722, &mut seq, 2000)?;

    if packets.len() != 2 {
        let mut packet_lengths = String::new();
        for i in 0..packets.len() {
            packet_lengths +=
                format!("Packet {} length {}\n", i, packets[i].payload.len()).as_str();
        }
        assert!(
            false,
            "Generated {} packets instead of 2\n{}",
            packets.len(),
            packet_lengths,
        );
    }
    Ok(())
}
