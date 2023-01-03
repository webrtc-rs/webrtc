use super::*;
use bytes::Bytes;
use std::io::BufReader;

/// build_ivf_container takes frames and prepends valid IVF file header
fn build_ivf_container(frames: &[Bytes]) -> Bytes {
    // Valid IVF file header taken from: https://github.com/webmproject/...
    // vp8-test-vectors/blob/master/vp80-00-comprehensive-001.ivf
    // Video Image Width      	- 176
    // Video Image Height    	- 144
    // Frame Rate Rate        	- 30000
    // Frame Rate Scale       	- 1000
    // Video Length in Frames	- 29
    // BitRate: 		 64.01 kb/s
    let header = Bytes::from_static(&[
        0x44, 0x4b, 0x49, 0x46, 0x00, 0x00, 0x20, 0x00, 0x56, 0x50, 0x38, 0x30, 0xb0, 0x00, 0x90,
        0x00, 0x30, 0x75, 0x00, 0x00, 0xe8, 0x03, 0x00, 0x00, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ]);

    let mut ivf = BytesMut::new();
    ivf.extend(header);

    for frame in frames {
        ivf.extend(frame);
    }

    ivf.freeze()
}

#[test]
fn test_ivf_reader_parse_valid_file_header() -> Result<()> {
    let ivf = build_ivf_container(&[]);

    let r = BufReader::new(&ivf[..]);
    let (_, header) = IVFReader::new(r)?;

    assert_eq!(&header.signature, b"DKIF", "signature is 'DKIF'");
    assert_eq!(header.version, 0, "version should be 0");
    assert_eq!(&header.four_cc, b"VP80", "FourCC should be 'VP80'");
    assert_eq!(header.width, 176, "width should be 176");
    assert_eq!(header.height, 144, "height should be 144");
    assert_eq!(
        header.timebase_denominator, 30000,
        "timebase denominator should be 30000"
    );
    assert_eq!(
        header.timebase_numerator, 1000,
        "timebase numerator should be 1000"
    );
    assert_eq!(header.num_frames, 29, "number of frames should be 29");
    assert_eq!(header.unused, 0, "bytes should be unused");

    Ok(())
}

#[test]
fn test_ivf_reader_parse_valid_frames() -> Result<()> {
    // Frame Length - 4
    // Timestamp - None
    // Frame Payload - 0xDEADBEEF
    let valid_frame1 = Bytes::from_static(&[
        0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xDE, 0xAD, 0xBE,
        0xEF,
    ]);

    // Frame Length - 12
    // Timestamp - None
    // Frame Payload - 0xDEADBEEFDEADBEEF
    let valid_frame2 = Bytes::from_static(&[
        0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xDE, 0xAD, 0xBE,
        0xEF, 0xDE, 0xAD, 0xBE, 0xEF, 0xDE, 0xAD, 0xBE, 0xEF,
    ]);

    let ivf = build_ivf_container(&[valid_frame1, valid_frame2]);
    let r = BufReader::new(&ivf[..]);
    let (mut reader, _) = IVFReader::new(r)?;

    // Parse Frame #1
    let (payload, header) = reader.parse_next_frame()?;

    assert_eq!(header.frame_size, 4, "Frame header frameSize should be 4");
    assert_eq!(payload.len(), 4, "Payload should be length 4");
    assert_eq!(
        payload,
        Bytes::from_static(&[0xDE, 0xAD, 0xBE, 0xEF,]),
        "Payload value should be 0xDEADBEEF"
    );
    assert_eq!(
        reader.bytes_read,
        IVF_FILE_HEADER_SIZE + IVF_FRAME_HEADER_SIZE + header.frame_size as usize
    );
    let previous_bytes_read = reader.bytes_read;

    // Parse Frame #2
    let (payload, header) = reader.parse_next_frame()?;

    assert_eq!(header.frame_size, 12, "Frame header frameSize should be 4");
    assert_eq!(payload.len(), 12, "Payload should be length 12");
    assert_eq!(
        payload,
        Bytes::from_static(&[
            0xDE, 0xAD, 0xBE, 0xEF, 0xDE, 0xAD, 0xBE, 0xEF, 0xDE, 0xAD, 0xBE, 0xEF,
        ]),
        "Payload value should be 0xDEADBEEFDEADBEEF"
    );
    assert_eq!(
        reader.bytes_read,
        previous_bytes_read + IVF_FRAME_HEADER_SIZE + header.frame_size as usize,
    );

    Ok(())
}

#[test]
fn test_ivf_reader_parse_incomplete_frame_header() -> Result<()> {
    // frame with 11-byte header (missing 1 byte)
    let incomplete_frame = Bytes::from_static(&[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]);

    let ivf = build_ivf_container(&[incomplete_frame]);
    let r = BufReader::new(&ivf[..]);
    let (mut reader, _) = IVFReader::new(r)?;

    // Parse Frame #1
    let result = reader.parse_next_frame();
    assert!(result.is_err(), "Expected Error but got Ok");

    Ok(())
}

#[test]
fn test_ivf_reader_parse_incomplete_frame_payload() -> Result<()> {
    // frame with header defining frameSize of 4
    // but only 2 bytes available (missing 2 bytes)
    let incomplete_frame = Bytes::from_static(&[
        0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xDE, 0xAD,
    ]);

    let ivf = build_ivf_container(&[incomplete_frame]);
    let r = BufReader::new(&ivf[..]);
    let (mut reader, _) = IVFReader::new(r)?;

    // Parse Frame #1
    let result = reader.parse_next_frame();
    assert!(result.is_err(), "Expected Error but got Ok");

    Ok(())
}

#[test]
fn test_ivf_reader_eof_when_no_frames_left() -> Result<()> {
    let ivf = build_ivf_container(&[]);
    let r = BufReader::new(&ivf[..]);
    let (mut reader, _) = IVFReader::new(r)?;

    let result = reader.parse_next_frame();
    assert!(result.is_err(), "Expected Error but got Ok");

    Ok(())
}
