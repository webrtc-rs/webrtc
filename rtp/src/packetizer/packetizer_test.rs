use super::*;
use crate::codecs::*;

use std::io::BufReader;

use util::Error;

#[test]
fn test_packetizer() -> Result<(), Error> {
    let multiplepayload = vec![0; 128];
    let mut reader = BufReader::new(multiplepayload.as_slice());

    //use the G722 payloader here, because it's very simple and all 0s is valid G722 data.
    let mut packetizer = PacketizerImpl::new(100, 98, 0x1234ABCD, 90000);

    let mut g722 = g722::G722Payloader;
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
