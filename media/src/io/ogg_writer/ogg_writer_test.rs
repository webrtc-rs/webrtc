use std::io::Cursor;

use super::*;
use crate::error::Error;

#[test]
fn test_ogg_writer_add_packet_and_close() -> Result<()> {
    let raw_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x00, 0x01, 0x00,
        0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);

    let mut valid_packet = rtp::packet::Packet {
        header: rtp::header::Header {
            marker: true,
            extension: true,
            extension_profile: 1,
            version: 2,
            //PayloadOffset:    20,
            payload_type: 111,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            padding: false,
            extensions: vec![],
            extensions_padding: 0,
        },
        payload: raw_pkt.slice(20..),
    };
    valid_packet
        .header
        .set_extension(0, Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))?;

    // The linter misbehave and thinks this code is the same as the tests in ivf-writer_test
    // nolint:dupl
    let add_packet_test_case = vec![
        (
            "OggWriter shouldn't be able to write an empty packet",
            "OggWriter should be able to close the file",
            rtp::packet::Packet::default(),
            Some(Error::ErrInvalidNilPacket),
        ),
        (
            "OggWriter should be able to write an Opus packet",
            "OggWriter should be able to close the file",
            valid_packet,
            None,
        ),
    ];

    for (msg1, _msg2, packet, err) in add_packet_test_case {
        let mut writer = OggWriter::new(Cursor::new(Vec::<u8>::new()), 4800, 2)?;
        let result = writer.write_rtp(&packet);
        if err.is_some() {
            assert!(result.is_err(), "{}", msg1);
            continue;
        } else {
            assert!(result.is_ok(), "{}", msg1);
        }
        writer.close()?;
    }

    Ok(())
}

#[test]
fn test_ogg_writer_add_packet() -> Result<()> {
    let raw_pkt = Bytes::from_iter(std::iter::repeat(0x45).take(235));

    let mut valid_packet = rtp::packet::Packet {
        header: rtp::header::Header {
            marker: true,
            extension: true,
            extension_profile: 1,
            version: 2,
            payload_type: 111,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            padding: false,
            extensions: vec![],
            extensions_padding: 0,
        },
        payload: raw_pkt,
    };
    valid_packet
        .header
        .set_extension(0, Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))?;

    let buffer = Cursor::new(Vec::<u8>::new());
    let mut writer = OggWriter::new(buffer, 48000, 2)?;
    let result = writer.write_rtp(&valid_packet);

    assert!(
        result.is_ok(),
        "OggWriter should be able to write an Opus packet smaller than 255 bytes"
    );
    assert!(
        writer.writer.into_inner()[126..128] == [1, 235],
        "OggWriter should be able to write an Opus packet smaller than 255 bytes"
    );

    Ok(())
}

#[test]
fn test_ogg_writer_add_packet_of_255() -> Result<()> {
    let raw_pkt = Bytes::from_iter(std::iter::repeat(0x45).take(255));

    let mut valid_packet = rtp::packet::Packet {
        header: rtp::header::Header {
            marker: true,
            extension: true,
            extension_profile: 1,
            version: 2,
            payload_type: 111,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            padding: false,
            extensions: vec![],
            extensions_padding: 0,
        },
        payload: raw_pkt,
    };
    valid_packet
        .header
        .set_extension(0, Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))?;

    let buffer = Cursor::new(Vec::<u8>::new());
    let mut writer = OggWriter::new(buffer, 48000, 2)?;
    let result = writer.write_rtp(&valid_packet);

    assert!(
        result.is_ok(),
        "OggWriter should be able to write an Opus packet of exactly 255"
    );
    assert!(
        writer.writer.into_inner()[126..128] == [1, 255],
        "OggWriter should be able to write an Opus packet of exactly 255"
    );

    Ok(())
}

#[test]
fn test_ogg_writer_add_large_packet() -> Result<()> {
    let raw_pkt = Bytes::from_iter(std::iter::repeat(0x45).take(1000));

    let mut valid_packet = rtp::packet::Packet {
        header: rtp::header::Header {
            marker: true,
            extension: true,
            extension_profile: 1,
            version: 2,
            payload_type: 111,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            padding: false,
            extensions: vec![],
            extensions_padding: 0,
        },
        payload: raw_pkt,
    };
    valid_packet
        .header
        .set_extension(0, Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))?;

    let buffer = Cursor::new(Vec::<u8>::new());
    let mut writer = OggWriter::new(buffer, 48000, 2)?;
    let result = writer.write_rtp(&valid_packet);

    assert!(
        result.is_ok(),
        "OggWriter should be able to write a large (> 255 bytes) Opus packet"
    );
    assert!(
        writer.writer.into_inner()[126..131] == [4, 255, 255, 255, 235],
        "OggWriter should be able to write multiple segments per page, for 1000 bytes, 4 segments of 255, 255, 255 and 235 long"
    );

    Ok(())
}

#[test]
fn test_ogg_writer_add_large_packet_with_multiple_of_255() -> Result<()> {
    let raw_pkt = Bytes::from_iter(std::iter::repeat(0x45).take(255 * 4));

    let mut valid_packet = rtp::packet::Packet {
        header: rtp::header::Header {
            marker: true,
            extension: true,
            extension_profile: 1,
            version: 2,
            payload_type: 111,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            padding: false,
            extensions: vec![],
            extensions_padding: 0,
        },
        payload: raw_pkt,
    };
    valid_packet
        .header
        .set_extension(0, Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))?;

    let buffer = Cursor::new(Vec::<u8>::new());
    let mut writer = OggWriter::new(buffer, 48000, 2)?;
    let result = writer.write_rtp(&valid_packet);

    assert!(
        result.is_ok(),
        "OggWriter should be able to write a large (> 255 bytes) Opus packet"
    );
    assert!(
        writer.writer.into_inner()[126..131] == [4, 255, 255, 255, 255],
        "OggWriter should be able to write multiple segments per page, for 1020 bytes, 4 segments of 255 each"
    );

    Ok(())
}
