//use super::*;

//use std::io::{BufReader, BufWriter};

//use util::Error;

//TODO: BenchmarkMarshal
//TODO: BenchmarkUnmarshal
/*
#[test]
fn test_basic() -> Result<(), Error> {
    let empty_bytes = vec![];
    let mut reader = BufReader::new(empty_bytes.as_slice());
    let result = Packet::unmarshal(&mut reader);
    if result.is_ok() {
        assert!(false, "Unmarshal did not error on zero length packet");
    }

    let raw_pkt = vec![
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x00, 0x01, 0x00,
        0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x98, 0x36, 0xbe, 0x88, 0x9e,
    ];
    let parsed_packet = Packet {
        header: Header {
            version: 2,
            padding: false,
            extension: true,
            marker: true,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            extension_profile: 1,
            //extension_payload: vec![0xFF, 0xFF, 0xFF, 0xFF],
            payload_offset: 20,
            ..Default::default()
        },
        payload: vec![0x98, 0x36, 0xbe, 0x88, 0x9e],
    };

    let mut reader = BufReader::new(raw_pkt.as_slice());
    let packet = Packet::unmarshal(&mut reader)?;
    assert_eq!(
        packet, parsed_packet,
        "TestBasic unmarshal: got {}, want {}",
        packet, parsed_packet
    );

    assert_eq!(packet.len(), raw_pkt.len(), "wrong computed marshal size");

    let mut raw: Vec<u8> = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        packet.marshal(&mut writer)?;
    }

    assert_eq!(
        raw.len(),
        raw_pkt.len(),
        "wrong raw marshal size {} vs {}",
        raw.len(),
        raw_pkt.len()
    );
    assert_eq!(
        raw, raw_pkt,
        "TestBasic marshal: got {:?}, want {:?}",
        raw, raw_pkt
    );

    Ok(())
}

#[test]
fn test_extension() -> Result<(), Error> {
    let missing_extension_pkt = vec![
        0x90, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82,
    ];
    let mut reader = BufReader::new(missing_extension_pkt.as_slice());
    let result = Packet::unmarshal(&mut reader);
    if result.is_ok() {
        assert!(
            false,
            "Unmarshal did not error on packet with missing extension data"
        );
    }

    let invalid_extension_length_pkt = vec![
        0x90, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x99, 0x99, 0x99,
        0x99,
    ];
    let mut reader = BufReader::new(invalid_extension_length_pkt.as_slice());
    let result = Packet::unmarshal(&mut reader);
    if result.is_ok() {
        assert!(
            false,
            "Unmarshal did not error on packet with invalid extension length"
        );
    }

    let packet = Packet {
        header: Header {
            extension: true,
            extension_profile: 3,
            //extension_payload: vec![0],
            ..Default::default()
        },
        payload: vec![],
    };

    let mut raw: Vec<u8> = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        let result = packet.marshal(&mut writer);
        if result.is_ok() {
            assert!(
                false,
                "Marshal did not error on packet with invalid extension length"
            );
        }
    }

    Ok(())
}
*/
// TODO: Benchmark RTP Marshal/Unmarshal
