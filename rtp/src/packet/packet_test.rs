// Silence warning on `..Default::default()` with no effect:
#![allow(clippy::needless_update)]

use bytes::{Bytes, BytesMut};

use super::*;
use crate::error::Result;

#[test]
fn test_basic() -> Result<()> {
    let mut empty_bytes = &vec![0u8; 0][..];
    let result = Packet::unmarshal(&mut empty_bytes);
    assert!(
        result.is_err(),
        "Unmarshal did not error on zero length packet"
    );

    let raw_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x00, 0x01, 0x00,
        0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);
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
            extensions: vec![Extension {
                id: 0,
                payload: Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]),
            }],
            ..Default::default()
        },
        payload: Bytes::from_static(&[0x98, 0x36, 0xbe, 0x88, 0x9e]),
    };
    let buf = &mut raw_pkt.clone();
    let packet = Packet::unmarshal(buf)?;
    assert_eq!(
        packet, parsed_packet,
        "TestBasic unmarshal: got {packet}, want {parsed_packet}"
    );
    assert_eq!(
        packet.header.marshal_size(),
        20,
        "wrong computed header marshal size"
    );
    assert_eq!(
        packet.marshal_size(),
        raw_pkt.len(),
        "wrong computed marshal size"
    );

    let raw = packet.marshal()?;
    let n = raw.len();
    assert_eq!(n, raw_pkt.len(), "wrong marshal size");

    assert_eq!(
        raw.len(),
        raw_pkt.len(),
        "wrong raw marshal size {} vs {}",
        raw.len(),
        raw_pkt.len()
    );
    assert_eq!(
        raw, raw_pkt,
        "TestBasic marshal: got {raw:?}, want {raw_pkt:?}"
    );

    Ok(())
}

#[test]
fn test_extension() -> Result<()> {
    let mut missing_extension_pkt = Bytes::from_static(&[
        0x90, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82,
    ]);
    let buf = &mut missing_extension_pkt;
    let result = Packet::unmarshal(buf);
    assert!(
        result.is_err(),
        "Unmarshal did not error on packet with missing extension data"
    );

    let mut invalid_extension_length_pkt = Bytes::from_static(&[
        0x90, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x99, 0x99, 0x99,
        0x99,
    ]);
    let buf = &mut invalid_extension_length_pkt;
    let result = Packet::unmarshal(buf);
    assert!(
        result.is_err(),
        "Unmarshal did not error on packet with invalid extension length"
    );

    let packet = Packet {
        header: Header {
            extension: true,
            extension_profile: 3,
            extensions: vec![Extension {
                id: 0,
                payload: Bytes::from_static(&[0]),
            }],
            ..Default::default()
        },
        payload: Bytes::from_static(&[]),
    };

    let mut raw = BytesMut::new();
    let result = packet.marshal_to(&mut raw);
    assert!(
        result.is_err(),
        "Marshal did not error on packet with invalid extension length"
    );
    if let Err(err) = result {
        assert_eq!(Error::ErrBufferTooSmall, err);
    }

    Ok(())
}

#[test]
fn test_padding() -> Result<()> {
    let raw_pkt = Bytes::from_static(&[
        0xa0, 0x60, 0x19, 0x58, 0x63, 0xff, 0x7d, 0x7c, 0x4b, 0x98, 0xd4, 0x0a, 0x67, 0x4d, 0x00,
        0x29, 0x9a, 0x64, 0x03, 0xc0, 0x11, 0x3f, 0x2c, 0xd4, 0x04, 0x04, 0x05, 0x00, 0x00, 0x03,
        0x03, 0xe8, 0x00, 0x00, 0xea, 0x60, 0x04, 0x00, 0x00, 0x03,
    ]);
    let buf = &mut raw_pkt.clone();
    let packet = Packet::unmarshal(buf)?;
    assert_eq!(&packet.payload[..], &raw_pkt[12..12 + 25]);

    let raw = packet.marshal()?;
    assert_eq!(raw, raw_pkt);

    Ok(())
}

#[test]
fn test_packet_marshal_unmarshal() -> Result<()> {
    let pkt = Packet {
        header: Header {
            extension: true,
            csrc: vec![1, 2],
            extension_profile: EXTENSION_PROFILE_TWO_BYTE,
            extensions: vec![
                Extension {
                    id: 1,
                    payload: Bytes::from_static(&[3, 4]),
                },
                Extension {
                    id: 2,
                    payload: Bytes::from_static(&[5, 6]),
                },
            ],
            ..Default::default()
        },
        payload: Bytes::from_static(&[0xFFu8; 15]),
        ..Default::default()
    };
    let mut raw = pkt.marshal()?;
    let p = Packet::unmarshal(&mut raw)?;

    assert_eq!(pkt, p);

    Ok(())
}

#[test]
fn test_rfc_8285_one_byte_extension() -> Result<()> {
    let raw_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x01, 0x50, 0xAA, 0x00, 0x00, 0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);
    let buf = &mut raw_pkt.clone();
    Packet::unmarshal(buf)?;

    let p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            extensions: vec![Extension {
                id: 5,
                payload: Bytes::from_static(&[0xAA]),
            }],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            ..Default::default()
        },
        payload: raw_pkt.slice(20..),
    };

    let dst = p.marshal()?;
    assert_eq!(dst, raw_pkt);

    Ok(())
}

#[test]
fn test_rfc_8285_one_byte_two_extension_of_two_bytes() -> Result<()> {
    //  0                   1                   2                   3
    //  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |       0xBE    |    0xDE       |           length=1            |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |  ID   | L=0   |     data      |  ID   |  L=0  |   data...
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    let raw_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x01, 0x10, 0xAA, 0x20, 0xBB, // Payload
        0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);
    let buf = &mut raw_pkt.clone();
    let p = Packet::unmarshal(buf)?;

    let ext1 = p.header.get_extension(1);
    let ext1_expect = Bytes::from_static(&[0xAA]);
    if let Some(ext1) = ext1 {
        assert_eq!(ext1, ext1_expect);
    } else {
        panic!("ext1 is none");
    }

    let ext2 = p.header.get_extension(2);
    let ext2_expect = Bytes::from_static(&[0xBB]);
    if let Some(ext2) = ext2 {
        assert_eq!(ext2, ext2_expect);
    } else {
        panic!("ext2 is none");
    }

    // Test Marshal
    let p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            extensions: vec![
                Extension {
                    id: 1,
                    payload: Bytes::from_static(&[0xAA]),
                },
                Extension {
                    id: 2,
                    payload: Bytes::from_static(&[0xBB]),
                },
            ],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            ..Default::default()
        },
        payload: raw_pkt.slice(20..),
    };

    let dst = p.marshal()?;
    assert_eq!(dst, raw_pkt);

    Ok(())
}

#[test]
fn test_rfc_8285_one_byte_multiple_extensions_with_padding() -> Result<()> {
    //  0                   1                   2                   3
    //  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |       0xBE    |    0xDE       |           length=3            |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |  ID   | L=0   |     data      |  ID   |  L=1  |   data...
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //       ...data   |    0 (pad)    |    0 (pad)    |  ID   | L=3   |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |                          data                                 |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

    let mut raw_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x00, 0x00, 0x33, 0xCC, 0xCC, 0xCC, 0xCC,
        // Payload
        0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);
    let buf = &mut raw_pkt;
    let packet = Packet::unmarshal(buf)?;
    let ext1 = packet
        .header
        .get_extension(1)
        .expect("Error getting header extension.");

    let ext1_expect = Bytes::from_static(&[0xAA]);
    assert_eq!(ext1, ext1_expect);

    let ext2 = packet
        .header
        .get_extension(2)
        .expect("Error getting header extension.");

    let ext2_expect = Bytes::from_static(&[0xBB, 0xBB]);
    assert_eq!(ext2, ext2_expect);

    let ext3 = packet
        .header
        .get_extension(3)
        .expect("Error getting header extension.");

    let ext3_expect = Bytes::from_static(&[0xCC, 0xCC, 0xCC, 0xCC]);
    assert_eq!(ext3, ext3_expect);

    let mut dst_buf: Vec<Vec<u8>> = vec![vec![0u8; 1000], vec![0xFF; 1000], vec![0xAA; 2]];

    let raw_pkg_marshal: [u8; 33] = [
        // padding is moved to the end by re-marshaling
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x33, 0xCC, 0xCC, 0xCC, 0xCC, 0x00, 0x00,
        // Payload
        0x98, 0x36, 0xbe, 0x88, 0x9e,
    ];

    let checker = |name: &str, buf: &mut [u8], p: &Packet| -> Result<()> {
        let size = p.marshal_to(buf)?;

        assert_eq!(
            &buf[..size],
            &raw_pkg_marshal[..],
            "Marshalled fields are not equal for {name}."
        );

        Ok(())
    };

    checker("CleanBuffer", &mut dst_buf[0], &packet)?;
    checker("DirtyBuffer", &mut dst_buf[1], &packet)?;

    let result = packet.marshal_to(&mut dst_buf[2]);
    assert!(result.is_err());
    if let Err(err) = result {
        assert_eq!(Error::ErrBufferTooSmall, err);
    }

    Ok(())
}

fn test_rfc_8285_one_byte_multiple_extension() -> Result<()> {
    //  0                   1                   2                   3
    //  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |       0xBE    |    0xDE       |           length=3            |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |  ID=1 | L=0   |     data      |  ID=2 |  L=1  |   data...
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //       ...data   |  ID=3 | L=3   |           data...
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //             ...data             |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    let raw_pkt = &[
        0x90u8, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x03, 0x10, 0xAA, 0x21, 0xBB, 0xBB, 0x33, 0xCC, 0xCC, 0xCC, 0xCC, 0x00, 0x00,
        // Payload
        0x98, 0x36, 0xbe, 0x88, 0x9e,
    ];

    let p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            extensions: vec![
                Extension {
                    id: 1,
                    payload: Bytes::from_static(&[0xAA]),
                },
                Extension {
                    id: 2,
                    payload: Bytes::from_static(&[0xBB, 0xBB]),
                },
                Extension {
                    id: 3,
                    payload: Bytes::from_static(&[0xCC, 0xCC]),
                },
            ],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload: raw_pkt[28..].into(),
    };

    let dst_data = p.marshal()?;
    assert_eq!(
        &dst_data[..],
        raw_pkt,
        "Marshal failed raw \nMarshaled:\n{dst_data:?}\nrawPkt:\n{raw_pkt:?}",
    );

    Ok(())
}

fn test_rfc_8285_two_byte_extension() -> Result<()> {
    let raw_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x10, 0x00, 0x00,
        0x07, 0x05, 0x18, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
        0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0x00, 0x00, 0x98,
        0x36, 0xbe, 0x88, 0x9e,
    ]);

    let _ = Packet::unmarshal(&mut raw_pkt.clone())?;

    let p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0x1000,
            extensions: vec![Extension {
                id: 5,
                payload: Bytes::from_static(&[
                    0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
                    0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
                ]),
            }],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload: raw_pkt.slice(44..),
    };

    let dst_data = p.marshal()?;
    assert_eq!(
        dst_data, raw_pkt,
        "Marshal failed raw \nMarshaled:\n{dst_data:?}\nrawPkt:\n{raw_pkt:?}"
    );
    Ok(())
}

fn test_rfc8285_two_byte_multiple_extension_with_padding() -> Result<()> {
    // 0                   1                   2                   3
    // 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |       0x10    |    0x00       |           length=3            |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |      ID=1     |     L=0       |     ID=2      |     L=1       |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |       data    |    0 (pad)    |       ID=3    |      L=4      |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |                          data                                 |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

    let mut raw_pkt = Bytes::from_static(&[
        0x90u8, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x10, 0x00, 0x00,
        0x03, 0x01, 0x00, 0x02, 0x01, 0xBB, 0x00, 0x03, 0x04, 0xCC, 0xCC, 0xCC, 0xCC, 0x98, 0x36,
        0xbe, 0x88, 0x9e,
    ]);

    let p = Packet::unmarshal(&mut raw_pkt)?;

    let ext = p.header.get_extension(1);
    let ext_expect = Some(Bytes::from_static(&[]));
    assert_eq!(
        ext, ext_expect,
        "Extension has incorrect data. Got: {ext:?}, Expected: {ext_expect:?}"
    );

    let ext = p.header.get_extension(2);
    let ext_expect = Some(Bytes::from_static(&[0xBB]));
    assert_eq!(
        ext, ext_expect,
        "Extension has incorrect data. Got: {ext:?}, Expected: {ext_expect:?}"
    );

    let ext = p.header.get_extension(3);
    let ext_expect = Some(Bytes::from_static(&[0xCC, 0xCC, 0xCC, 0xCC]));
    assert_eq!(
        ext, ext_expect,
        "Extension has incorrect data. Got: {ext:?}, Expected: {ext_expect:?}"
    );

    Ok(())
}

fn test_rfc8285_two_byte_multiple_extension_with_large_extension() -> Result<()> {
    // 0                   1                   2                   3
    // 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |       0x10    |    0x00       |           length=3            |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |      ID=1     |     L=0       |     ID=2      |     L=1       |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |       data    |       ID=3    |      L=17      |    data...
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //                            ...data...
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //                            ...data...
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //                            ...data...
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //                            ...data...                           |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

    let raw_pkt = Bytes::from_static(&[
        0x90u8, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x10, 0x00, 0x00,
        0x06, 0x01, 0x00, 0x02, 0x01, 0xBB, 0x03, 0x11, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
        0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, // Payload
        0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);

    let p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0x1000,
            extensions: vec![
                Extension {
                    id: 1,
                    payload: Bytes::from_static(&[]),
                },
                Extension {
                    id: 2,
                    payload: Bytes::from_static(&[0xBB]),
                },
                Extension {
                    id: 3,
                    payload: Bytes::from_static(&[
                        0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
                        0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
                    ]),
                },
            ],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload: raw_pkt.slice(40..),
    };

    let dst_data = p.marshal()?;
    assert_eq!(
        dst_data,
        raw_pkt[..],
        "Marshal failed raw \nMarshaled: {dst_data:?}, \nraw_pkt:{raw_pkt:?}"
    );

    Ok(())
}

fn test_rfc8285_get_extension_returns_nil_when_extension_disabled() -> Result<()> {
    let payload = Bytes::from_static(&[
        // Payload
        0x98u8, 0x36, 0xbe, 0x88, 0x9e,
    ]);

    let p = Packet {
        header: Header {
            marker: true,
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    let res = p.header.get_extension(1);
    assert!(
        res.is_none(),
        "Should return none on get_extension when header extension is false"
    );

    Ok(())
}

fn test_rfc8285_del_extension() -> Result<()> {
    let payload = Bytes::from_static(&[
        // Payload
        0x98u8, 0x36, 0xbe, 0x88, 0x9e,
    ]);
    let mut p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            extensions: vec![Extension {
                id: 1,
                payload: Bytes::from_static(&[0xAA]),
            }],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    let ext = p.header.get_extension(1);
    assert!(ext.is_some(), "Extension should exist");

    p.header.del_extension(1)?;

    let ext = p.header.get_extension(1);
    assert!(ext.is_none(), "Extension should not exist");

    let err = p.header.del_extension(1);
    assert!(
        err.is_err(),
        "Should return error when deleting extension that doesnt exist"
    );

    Ok(())
}

fn test_rfc8285_get_extension_ids() {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            extensions: vec![
                Extension {
                    id: 1,
                    payload: Bytes::from_static(&[0xAA]),
                },
                Extension {
                    id: 2,
                    payload: Bytes::from_static(&[0xBB]),
                },
            ],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    let ids = p.header.get_extension_ids();
    assert!(!ids.is_empty(), "Extensions should exist");

    assert_eq!(
        ids.len(),
        p.header.extensions.len(),
        "The number of IDs should be equal to the number of extensions, want={}, hanve{}",
        ids.len(),
        p.header.extensions.len()
    );

    for id in ids {
        let ext = p.header.get_extension(id);
        assert!(ext.is_some(), "Extension should exist for id: {id}")
    }
}

fn test_rfc8285_get_extension_ids_return_empty_when_extension_disabled() {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let p = Packet {
        header: Header {
            marker: true,
            extension: false,
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    let ids = p.header.get_extension_ids();
    assert!(ids.is_empty(), "Extensions should not exist");
}

fn test_rfc8285_del_extension_returns_error_when_extensions_disabled() {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let mut p = Packet {
        header: Header {
            marker: true,
            extension: false,
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    let ids = p.header.del_extension(1);
    assert!(
        ids.is_err(),
        "Should return error on del_extension when header extension field is false"
    );
}

fn test_rfc8285_one_byte_set_extension_should_enable_extension_when_adding() {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let mut p = Packet {
        header: Header {
            marker: true,
            extension: false,
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    let extension = Bytes::from_static(&[0xAAu8, 0xAA]);
    let result = p.header.set_extension(1, extension.clone());
    assert!(result.is_ok(), "Error setting extension");

    assert!(p.header.extension, "Extension should be set to true");
    assert_eq!(
        p.header.extension_profile, 0xBEDE,
        "Extension profile should be set to 0xBEDE"
    );
    assert_eq!(
        p.header.extensions.len(),
        1,
        "Extensions len should be set to 1"
    );
    assert_eq!(
        p.header.get_extension(1),
        Some(extension),
        "Extension value is not set"
    )
}

fn test_rfc8285_set_extension_should_set_correct_extension_profile_for_16_byte_extension() {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let mut p = Packet {
        header: Header {
            marker: true,
            extension: false,
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    let extension = Bytes::from_static(&[
        0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
        0xAA,
    ]);

    let res = p.header.set_extension(1, extension);
    assert!(res.is_ok(), "Error setting extension");

    assert_eq!(
        p.header.extension_profile, 0xBEDE,
        "Extension profile should be 0xBEDE"
    );
}

fn test_rfc8285_set_extension_should_update_existing_extension() -> Result<()> {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let mut p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            extensions: vec![Extension {
                id: 1,
                payload: Bytes::from_static(&[0xAA]),
            }],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    assert_eq!(
        p.header.get_extension(1),
        Some([0xAA][..].into()),
        "Extension value not initialized properly"
    );

    let extension = Bytes::from_static(&[0xBBu8]);
    p.header.set_extension(1, extension.clone())?;

    assert_eq!(
        p.header.get_extension(1),
        Some(extension),
        "Extension value was not set"
    );

    Ok(())
}

fn test_rfc8285_one_byte_set_extension_should_error_when_invalid_id_provided() {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let mut p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            extensions: vec![Extension {
                id: 1,
                payload: Bytes::from_static(&[0xAA]),
            }],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    assert!(
        p.header
            .set_extension(0, Bytes::from_static(&[0xBBu8]))
            .is_err(),
        "set_extension did not error on invalid id"
    );
    assert!(
        p.header
            .set_extension(15, Bytes::from_static(&[0xBBu8]))
            .is_err(),
        "set_extension did not error on invalid id"
    );
}

fn test_rfc8285_one_byte_extension_terminate_processing_when_reserved_id_encountered() -> Result<()>
{
    let reserved_id_pkt = Bytes::from_static(&[
        0x90u8, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE, 0x00,
        0x01, 0xF0, 0xAA, 0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);

    let p = Packet::unmarshal(&mut reserved_id_pkt.clone())?;

    assert_eq!(
        p.header.extensions.len(),
        0,
        "Extension should be empty for invalid ID"
    );

    let payload = reserved_id_pkt.slice(17..);
    assert_eq!(p.payload, payload, "p.payload must be same as payload");

    Ok(())
}

fn test_rfc8285_one_byte_set_extension_should_error_when_payload_too_large() {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let mut p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            extensions: vec![Extension {
                id: 1,
                payload: Bytes::from_static(&[0xAAu8]),
            }],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    let res = p.header.set_extension(
        1,
        Bytes::from_static(&[
            0xBBu8, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB,
        ]),
    );

    assert!(
        res.is_err(),
        "set_extension did not error on too large payload"
    );
}

fn test_rfc8285_two_bytes_set_extension_should_enable_extension_when_adding() -> Result<()> {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let mut p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    let extension = Bytes::from_static(&[
        0xAAu8, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
        0xAA, 0xAA,
    ]);

    p.header.set_extension(1, extension.clone())?;

    assert!(p.header.extension, "Extension should be set to true");
    assert_eq!(
        p.header.extension_profile, 0x1000,
        "Extension profile should be set to 0xBEDE"
    );
    assert_eq!(
        p.header.extensions.len(),
        1,
        "Extensions should be set to 1"
    );
    assert_eq!(
        p.header.get_extension(1),
        Some(extension),
        "Extension value is not set"
    );

    Ok(())
}

fn test_rfc8285_two_byte_set_extension_should_update_existing_extension() -> Result<()> {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let mut p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0x1000,
            extensions: vec![Extension {
                id: 1,
                payload: Bytes::from_static(&[0xAA]),
            }],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    assert_eq!(
        p.header.get_extension(1),
        Some(Bytes::from_static(&[0xAA])),
        "Extension value not initialized properly"
    );

    let extension = Bytes::from_static(&[
        0xBBu8, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
        0xBB, 0xBB,
    ]);

    p.header.set_extension(1, extension.clone())?;

    assert_eq!(p.header.get_extension(1), Some(extension));

    Ok(())
}

fn test_rfc8285_two_byte_set_extension_should_error_when_payload_too_large() {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let mut p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0xBEDE,
            extensions: vec![Extension {
                id: 1,
                payload: Bytes::from_static(&[0xAA]),
            }],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    let res = p.header.set_extension(
        1,
        Bytes::from_static(&[
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB,
        ]),
    );

    assert!(
        res.is_err(),
        "Set extension did not error on too large payload"
    );
}

fn test_rfc3550_set_extension_should_error_when_non_zero() -> Result<()> {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let mut p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0x1111,
            extensions: vec![Extension {
                id: 1,
                payload: Bytes::from_static(&[0xAA]),
            }],
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    p.header.set_extension(0, Bytes::from_static(&[0xBB]))?;
    let res = p.header.get_extension(0);
    assert_eq!(
        res,
        Some(Bytes::from_static(&[0xBB])),
        "p.get_extension returned incorrect value"
    );

    Ok(())
}

fn test_rfc3550_set_extension_should_error_when_setting_non_zero_id() {
    let payload = Bytes::from_static(&[0x98u8, 0x36, 0xbe, 0x88, 0x9e]);

    let mut p = Packet {
        header: Header {
            marker: true,
            extension: true,
            extension_profile: 0x1111,
            version: 2,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            ..Default::default()
        },
        payload,
        ..Default::default()
    };

    let res = p.header.set_extension(1, Bytes::from_static(&[0xBB]));
    assert!(res.is_err(), "set_extension did not error on invalid id");
}

use std::collections::HashMap;

struct Cases {
    input: Bytes,
    err: Error,
}

fn test_unmarshal_error_handling() {
    let mut cases = HashMap::new();

    cases.insert(
        "ShortHeader",
        Cases {
            input: Bytes::from_static(&[
                0x80, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, // timestamp
                0x1c, 0x64, 0x27, // SSRC (one byte missing)
            ]),
            err: Error::ErrHeaderSizeInsufficient,
        },
    );

    cases.insert(
        "MissingCSRC",
        Cases {
            input: Bytes::from_static(&[
                0x81, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, // timestamp
                0x1c, 0x64, 0x27, 0x82, // SSRC
            ]),
            err: Error::ErrHeaderSizeInsufficient,
        },
    );

    cases.insert(
        "MissingExtension",
        Cases {
            input: Bytes::from_static(&[
                0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, // timestamp
                0x1c, 0x64, 0x27, 0x82, // SSRC
            ]),
            err: Error::ErrHeaderSizeInsufficientForExtension,
        },
    );

    cases.insert(
        "MissingExtensionData",
        Cases {
            input: Bytes::from_static(&[
                0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, // timestamp
                0x1c, 0x64, 0x27, 0x82, // SSRC
                0xBE, 0xDE, 0x00, 0x03, // specified to have 3 extensions, but actually not
            ]),
            err: Error::ErrHeaderSizeInsufficientForExtension,
        },
    );

    cases.insert(
        "MissingExtensionDataPayload",
        Cases {
            input: Bytes::from_static(&[
                0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, // timestamp
                0x1c, 0x64, 0x27, 0x82, // SSRC
                0xBE, 0xDE, 0x00, 0x01, // have 1 extension
                0x12,
                0x00, // length of the payload is expected to be 3, but actually have only 1
            ]),
            err: Error::ErrHeaderSizeInsufficientForExtension,
        },
    );

    for (name, mut test_case) in cases.drain() {
        let result = Header::unmarshal(&mut test_case.input);
        let err = result.err().unwrap();
        assert_eq!(
            test_case.err, err,
            "Expected :{:?}, found: {:?} for testcase {}",
            test_case.err, err, name
        )
    }
}

fn test_round_trip() -> Result<()> {
    let raw_pkt = Bytes::from_static(&[
        0x00u8, 0x10, 0x23, 0x45, 0x12, 0x34, 0x45, 0x67, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22,
        0x33, 0x44, 0x55, 0x66, 0x77,
    ]);

    let payload = raw_pkt.slice(12..);

    let p = Packet::unmarshal(&mut raw_pkt.clone())?;

    assert_eq!(
        payload, p.payload,
        "p.payload must be same as payload.\n p.payload: {:?},\nraw_pkt: {:?}",
        p.payload, payload
    );

    let buf = p.marshal()?;

    assert_eq!(
        raw_pkt, buf,
        "buf must be the same as raw_pkt. \n buf: {buf:?},\nraw_pkt: {raw_pkt:?}",
    );
    assert_eq!(
        payload, p.payload,
        "p.payload must be the same as payload. \n payload: {:?},\np.payload: {:?}",
        payload, p.payload,
    );

    Ok(())
}
