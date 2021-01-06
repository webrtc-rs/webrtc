// use super::*;
// use crate::codecs::*;

// use chrono::prelude::*;
// use std::io::BufReader;
// use std::time::Duration;

#[test]
fn test_packetizer() -> Result<(), Error> {
    let multiplepayload = vec![0; 128];
    let mut reader = BufReader::new(multiplepayload.as_slice());

// #[test]
// fn test_packetizer() -> Result<(), Error> {
//     let multiplepayload = vec![0; 128];
//     let mut reader = BufReader::new(multiplepayload.as_slice());

//     //use the G722 payloader here, because it's very simple and all 0s is valid G722 data.
//     let mut packetizer = PacketizerImpl::new(100, 98, 0x1234ABCD, 90000);

//     let mut g722 = g722::G722Payloader;
//     let mut seq = new_random_sequencer();
//     let packets = packetizer.packetize(&mut reader, &mut g722, &mut seq, 2000)?;

//     if packets.len() != 2 {
//         let mut packet_lengths = String::new();
//         for i in 0..packets.len() {
//             packet_lengths +=
//                 format!("Packet {} length {}\n", i, packets[i].payload.len()).as_str();
//         }
//         assert!(
//             false,
//             "Generated {} packets instead of 2\n{}",
//             packets.len(),
//             packet_lengths,
//         );
//     }
//     Ok(())
// }

// fn fixed_time_gen() -> Duration {
//     let loc = FixedOffset::west(5 * 60 * 60); // UTC-5
//     let t = loc.ymd(1985, 6, 23).and_hms_nano(4, 0, 0, 0);
//     Duration::from_nanos(t.timestamp_nanos() as u64)
// }

// #[test]
// fn test_packetizer_abs_send_time() -> Result<(), Error> {
//     //use the G722 payloader here, because it's very simple and all 0s is valid G722 data.
//     let mut pktizer = PacketizerImpl::new(100, 98, 0x1234ABCD, 90000);
//     pktizer.timestamp = 45678;
//     pktizer.time_gen = Some(fixed_time_gen);
//     pktizer.enable_abs_send_time(1);

//     let payload = vec![0x11, 0x12, 0x13, 0x14];
//     let mut reader = BufReader::new(payload.as_slice());

//     let mut g722 = g722::G722Payloader;
//     let mut seq = new_fixed_sequencer(1234);
//     let packets = pktizer.packetize(&mut reader, &mut g722, &mut seq, 2000)?;

//     let expected = Packet {
//         header: Header {
//             version: 2,
//             padding: false,
//             extension: true,
//             marker: true,
//             payload_offset: 0, // not set by Packetize() at now
//             payload_type: 98,
//             sequence_number: 1234,
//             timestamp: 45678,
//             ssrc: 0x1234ABCD,
//             csrc: vec![],
//             extension_profile: 0xBEDE,
//             extensions: vec![Extension {
//                 id: 1,
//                 payload: vec![0x40, 0, 0],
//             }],
//         },
//         payload: vec![0x11, 0x12, 0x13, 0x14],
//     };

//     if packets.len() != 1 {
//         assert!(false, "Generated {} packets instead of 1", packets.len())
//     }

//     assert_eq!(expected, packets[0]);

//     Ok(())
// }
