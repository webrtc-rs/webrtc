use super::*;
use crate::error::Error;
use std::io::Cursor;

#[test]
fn test_ivf_writer_add_packet_and_close() -> Result<()> {
    // Construct valid packet
    let raw_valid_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x00, 0x01, 0x00,
        0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x98, 0x36, 0xbe, 0x89, 0x9e,
    ]);

    let mut valid_packet = rtp::packet::Packet {
        header: rtp::header::Header {
            marker: true,
            extension: true,
            extension_profile: 1,
            version: 2,
            //payloadOffset:    20,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            padding: false,
            extensions: vec![],
        },
        payload: raw_valid_pkt.slice(20..),
    };
    valid_packet
        .header
        .set_extension(0, Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))?;

    // Construct mid partition packet
    let raw_mid_part_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x00, 0x01, 0x00,
        0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x88, 0x36, 0xbe, 0x89, 0x9e,
    ]);

    let mut mid_part_packet = rtp::packet::Packet {
        header: rtp::header::Header {
            marker: true,
            extension: true,
            extension_profile: 1,
            version: 2,
            //PayloadOffset:    20,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            padding: raw_mid_part_pkt.len() % 4 != 0,
            extensions: vec![],
        },
        payload: raw_mid_part_pkt.slice(20..),
    };
    mid_part_packet
        .header
        .set_extension(0, Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))?;

    // Construct keyframe packet
    let raw_keyframe_pkt = Bytes::from_static(&[
        0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x00, 0x01, 0x00,
        0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]);

    let mut keyframe_packet = rtp::packet::Packet {
        header: rtp::header::Header {
            marker: true,
            extension: true,
            extension_profile: 1,
            version: 2,
            //PayloadOffset:    20,
            payload_type: 96,
            sequence_number: 27023,
            timestamp: 3653407706,
            ssrc: 476325762,
            csrc: vec![],
            padding: raw_keyframe_pkt.len() % 4 != 0,
            extensions: vec![],
        },
        payload: raw_keyframe_pkt.slice(20..),
    };
    keyframe_packet
        .header
        .set_extension(0, Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))?;

    // Check valid packet parameters
    let mut vp8packet = rtp::codecs::vp8::Vp8Packet::default();
    let payload = vp8packet.depacketize(&valid_packet.payload)?;
    assert_eq!(1, vp8packet.s, "Start packet S value should be 1");
    assert_eq!(
        payload[0] & 0x01,
        1,
        "Non Keyframe packet P value should be 1"
    );

    // Check mid partition packet parameters
    let mut vp8packet = rtp::codecs::vp8::Vp8Packet::default();
    let payload = vp8packet.depacketize(&mid_part_packet.payload)?;
    assert_eq!(vp8packet.s, 0, "Mid Partition packet S value should be 0");
    assert_eq!(
        payload[0] & 0x01,
        1,
        "Non Keyframe packet P value should be 1"
    );

    // Check keyframe packet parameters
    let mut vp8packet = rtp::codecs::vp8::Vp8Packet::default();
    let payload = vp8packet.depacketize(&keyframe_packet.payload)?;
    assert_eq!(vp8packet.s, 1, "Start packet S value should be 1");
    assert_eq!(payload[0] & 0x01, 0, "Keyframe packet P value should be 0");

    let add_packet_test_case = vec![
        (
            "IVFWriter shouldn't be able to write something an empty packet",
            "IVFWriter should be able to close the file",
            rtp::packet::Packet::default(),
            Some(Error::ErrInvalidNilPacket),
            false,
            0,
        ),
        (
            "IVFWriter should be able to write an IVF packet",
            "IVFWriter should be able to close the file",
            valid_packet.clone(),
            None,
            false,
            1,
        ),
        (
            "IVFWriter should be able to write a Keframe IVF packet",
            "IVFWriter should be able to close the file",
            keyframe_packet,
            None,
            true,
            2,
        ),
    ];

    let header = IVFFileHeader {
        signature: *b"DKIF",      // DKIF
        version: 0,               // version
        header_size: 32,          // Header size
        four_cc: *b"VP80",        // FOURCC
        width: 640,               // Width in pixels
        height: 480,              // Height in pixels
        timebase_denominator: 30, // Framerate denominator
        timebase_numerator: 1,    // Framerate numerator
        num_frames: 900,          // Frame count, will be updated on first Close() call
        unused: 0,                // Unused
    };

    for (msg1, _msg2, packet, err, seen_key_frame, count) in add_packet_test_case {
        let mut writer = IVFWriter::new(Cursor::new(Vec::<u8>::new()), &header)?;
        assert!(
            !writer.seen_key_frame,
            "Writer's seenKeyFrame should initialize false"
        );
        assert_eq!(writer.count, 0, "Writer's packet count should initialize 0");
        let result = writer.write_rtp(&packet);
        if err.is_some() {
            assert!(result.is_err(), "{}", msg1);
            continue;
        } else {
            assert!(result.is_ok(), "{}", msg1);
        }

        assert_eq!(seen_key_frame, writer.seen_key_frame, "{msg1} failed");
        if count == 1 {
            assert_eq!(writer.count, 0);
        } else if count == 2 {
            assert_eq!(writer.count, 1);
        }

        writer.write_rtp(&mid_part_packet)?;
        if count == 1 {
            assert_eq!(writer.count, 0);
        } else if count == 2 {
            assert_eq!(writer.count, 1);

            writer.write_rtp(&valid_packet)?;
            assert_eq!(writer.count, 2);
        }

        writer.close()?;
    }

    Ok(())
}
