use std::io::Cursor;

use super::*;

#[test]
fn test_data_does_not_start_with_h264header() -> Result<()> {
    let test_function = |input: &[u8]| {
        let mut reader = H264Reader::new(Cursor::new(input), 1_048_576);
        if let Err(err) = reader.next_nal() {
            assert_eq!(err, Error::ErrDataIsNotH264Stream);
        } else {
            panic!();
        }
    };

    test_function(&[2]);
    test_function(&[0, 2]);
    test_function(&[0, 0, 2]);
    test_function(&[0, 0, 2, 0]);
    test_function(&[0, 0, 0, 2]);

    Ok(())
}

#[test]
fn test_parse_header() -> Result<()> {
    let h264bytes = &[0x0, 0x0, 0x1, 0xAB];
    let mut reader = H264Reader::new(Cursor::new(h264bytes), 1_048_576);

    let nal = reader.next_nal()?;

    assert_eq!(nal.data.len(), 1);
    assert!(nal.forbidden_zero_bit);
    assert_eq!(nal.picture_order_count, 0);
    assert_eq!(nal.ref_idc, 1);
    assert_eq!(NalUnitType::EndOfStream, nal.unit_type);

    Ok(())
}

#[test]
fn test_eof() -> Result<()> {
    let test_function = |input: &[u8]| {
        let mut reader = H264Reader::new(Cursor::new(input), 1_048_576);
        if let Err(err) = reader.next_nal() {
            assert_eq!(Error::ErrIoEOF, err);
        } else {
            panic!();
        }
    };

    test_function(&[0, 0, 0, 1]);
    test_function(&[0, 0, 1]);
    test_function(&[]);

    Ok(())
}

#[test]
fn test_skip_sei() -> Result<()> {
    let h264bytes = &[
        0x0, 0x0, 0x0, 0x1, 0xAA, 0x0, 0x0, 0x0, 0x1, 0x6, // SEI
        0x0, 0x0, 0x0, 0x1, 0xAB,
    ];

    let mut reader = H264Reader::new(Cursor::new(h264bytes), 1_048_576);

    let nal = reader.next_nal()?;
    assert_eq!(nal.data[0], 0xAA);

    let nal = reader.next_nal()?;
    assert_eq!(nal.data[0], 0xAB);

    Ok(())
}

#[test]
fn test_issue1734_next_nal() -> Result<()> {
    let tests: Vec<&[u8]> = vec![
        &[0x00, 0x00, 0x010, 0x00, 0x00, 0x01, 0x00, 0x00, 0x01],
        &[0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x01],
    ];

    for test in tests {
        let mut reader = H264Reader::new(Cursor::new(test), 1_048_576);

        // Just make sure it doesn't crash
        while reader.next_nal().is_ok() {
            //do nothing
        }
    }

    Ok(())
}

#[test]
fn test_trailing01after_start_code() -> Result<()> {
    let test = vec![0x0, 0x0, 0x0, 0x1, 0x01, 0x0, 0x0, 0x0, 0x1, 0x01];
    let mut r = H264Reader::new(Cursor::new(test), 1_048_576);

    for _ in 0..=1 {
        let _nal = r.next_nal()?;
    }

    Ok(())
}
