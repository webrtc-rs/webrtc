use std::io::Cursor;

use bytes::Bytes;

use super::*;

#[test]
fn test_is_key_frame() -> Result<()> {
    let tests = vec![
        (
            "When given a non-keyframe; it should return false",
            vec![0x27, 0x90, 0x90],
            false,
        ),
        (
            "When given a SPS packetized with STAP-A;; it should return true",
            vec![
                0x38, 0x00, 0x03, 0x27, 0x90, 0x90, 0x00, 0x05, 0x28, 0x90, 0x90, 0x90, 0x90,
            ],
            true,
        ),
        (
            "When given a SPS with no packetization; it should return true",
            vec![0x27, 0x90, 0x90, 0x00],
            true,
        ),
    ];

    for (name, payload, want) in tests {
        let got = is_key_frame(&payload);
        assert_eq!(got, want, "{name} failed");
    }

    Ok(())
}

#[test]
fn test_write_rtp() -> Result<()> {
    let tests = vec![
        (
            "When given an empty payload; it should return nil",
            vec![],
            false,
            vec![],
            false,
        ),
        (
            "When no keyframe is defined; it should discard the packet",
            vec![0x25, 0x90, 0x90],
            false,
            vec![],
            false,
        ),
        (
            "When a valid Single NAL Unit packet is given; it should unpack it without error",
            vec![0x27, 0x90, 0x90],
            true,
            vec![0x00, 0x00, 0x00, 0x01, 0x27, 0x90, 0x90],
            false,
        ),
        (
            "When a valid STAP-A packet is given; it should unpack it without error",
            vec![
                0x38, 0x00, 0x03, 0x27, 0x90, 0x90, 0x00, 0x05, 0x28, 0x90, 0x90, 0x90, 0x90,
            ],
            true,
            vec![
                0x00, 0x00, 0x00, 0x01, 0x27, 0x90, 0x90, 0x00, 0x00, 0x00, 0x01, 0x28, 0x90, 0x90,
                0x90, 0x90,
            ],
            false,
        ),
    ];

    for (_name, payload, has_key_frame, want_bytes, _reuse) in tests {
        let mut writer = vec![];
        {
            let w = Cursor::new(&mut writer);
            let mut h264writer = H264Writer::new(w);
            h264writer.has_key_frame = has_key_frame;

            let packet = rtp::packet::Packet {
                payload: Bytes::from(payload),
                ..Default::default()
            };

            h264writer.write_rtp(&packet)?;
            h264writer.close()?;
        }

        assert_eq!(writer, want_bytes);
    }

    Ok(())
}

#[test]
fn test_write_rtp_fu() -> Result<()> {
    let tests = vec![
        vec![0x3C, 0x85, 0x90, 0x90, 0x90],
        vec![0x3C, 0x45, 0x90, 0x90, 0x90],
    ];

    let want_bytes = vec![
        0x00, 0x00, 0x00, 0x01, 0x25, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90,
    ];

    let mut writer = vec![];
    {
        let w = Cursor::new(&mut writer);
        let mut h264writer = H264Writer::new(w);
        h264writer.has_key_frame = true;

        for payload in tests {
            let packet = rtp::packet::Packet {
                payload: Bytes::from(payload),
                ..Default::default()
            };

            h264writer.write_rtp(&packet)?;
        }
        h264writer.close()?;
    }
    assert_eq!(writer, want_bytes);

    Ok(())
}
