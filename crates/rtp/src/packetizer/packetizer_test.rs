// #[cfg(test)]
// mod test {
//     use crate::codecs;
//     use crate::{header::Extension, packet::Packet, packetizer::*, sequence};
//     use chrono::prelude::*;
//     use std::any::Any;
//     #[test]
//     fn test_packetizer() {
//         let mut multiplepayload = vec![0; 128].as_slice().into();

//         //use the G722 payloader here, because it's very simple and all 0s is valid G722 data.
//         let mut packetizer = new_packetizer(
//             100,
//             98,
//             0x1234ABCD,
//             90000,
//             Box::new(codecs::g722::G722Payloader),
//             Box::new(sequence::new_random_sequencer()),
//         );

//         let packets = packetizer.packetize(&mut multiplepayload, 2000).unwrap();

//         if packets.len() != 2 {
//             let mut packet_lengths = String::new();
//             for i in 0..packets.len() {
//                 packet_lengths +=
//                     format!("Packet {} length {}\n", i, packets[i].payload.len()).as_str();
//             }

//             panic!(
//                 "Generated {} packets instead of 2\n{}",
//                 packets.len(),
//                 packet_lengths,
//             );
//         }
//     }

//     #[test]
//     fn test_packetizer_abs_send_time() {
//         //use the G722 payloader here, because it's very simple and all 0s is valid G722 data.
//         let mut pktizer = new_packetizer(
//             100,
//             98,
//             0x1234ABCD,
//             90000,
//             Box::new(codecs::g722::G722Payloader),
//             Box::new(sequence::new_fixed_sequencer(1234)),
//         );

//         match (&mut pktizer as &mut dyn Any).downcast_mut::<Packetizer>() {
//             Some(e) => {
//                 let mut e: &mut Packetizer = e;

//                 e.timestamp = 45678;
//                 e.time_gen = Some(|| fixed_time_gen())
//             }

//             None => panic!("failed to cast to packet type"),
//         };

//         pktizer.enable_abs_send_time(1);

//         let mut payload = vec![0x11u8, 0x12, 0x13, 0x14].as_slice().into();

//         let packets = pktizer.packetize(&mut payload, 2000).unwrap();

//         let expected = Packet {
//             header: Header {
//                 version: 2,
//                 padding: false,
//                 extension: true,
//                 marker: true,
//                 payload_type: 98,
//                 sequence_number: 1234,
//                 timestamp: 45678,
//                 ssrc: 0x1234ABCD,
//                 csrc: vec![],
//                 extension_profile: 0xBEDE,
//                 extensions: vec![Extension {
//                     id: 1,
//                     payload: vec![0x40, 0, 0][..].into(),
//                 }],
//             },
//             payload: vec![0x11, 0x12, 0x13, 0x14][..].into(),
//             ..Default::default()
//         };

//         if packets.len() != 1 {
//             assert!(false, "Generated {} packets instead of 1", packets.len())
//         }

//         assert_eq!(expected, packets[0]);
//     }

//     fn fixed_time_gen() -> std::time::Duration {
//         let loc = FixedOffset::west(5 * 60 * 60); // UTC-5
//         let t = loc.ymd(1985, 6, 23).and_hms_nano(4, 0, 0, 0);
//         Duration::from_nanos(t.timestamp_nanos() as u64)
//     }
// }
