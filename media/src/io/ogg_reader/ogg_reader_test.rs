use bytes::Bytes;

use super::*;

// generates a valid ogg file that can be used for tests
fn build_ogg_container() -> Vec<u8> {
    vec![
        0x4f, 0x67, 0x67, 0x53, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x8e,
        0x9b, 0x20, 0xaa, 0x00, 0x00, 0x00, 0x00, 0x61, 0xee, 0x61, 0x17, 0x01, 0x13, 0x4f, 0x70,
        0x75, 0x73, 0x48, 0x65, 0x61, 0x64, 0x01, 0x02, 0x00, 0x0f, 0x80, 0xbb, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x4f, 0x67, 0x67, 0x53, 0x00, 0x00, 0xda, 0x93, 0xc2, 0xd9, 0x00, 0x00, 0x00,
        0x00, 0x8e, 0x9b, 0x20, 0xaa, 0x02, 0x00, 0x00, 0x00, 0x49, 0x97, 0x03, 0x37, 0x01, 0x05,
        0x98, 0x36, 0xbe, 0x88, 0x9e,
    ]
}

#[test]
fn test_ogg_reader_parse_valid_header() -> Result<()> {
    let ogg = build_ogg_container();
    let r = Cursor::new(&ogg);
    let (_reader, header) = OggReader::new(r, true)?;

    assert_eq!(header.channel_map, 0);
    assert_eq!(header.channels, 2);
    assert_eq!(header.output_gain, 0);
    assert_eq!(header.pre_skip, 0xf00);
    assert_eq!(header.sample_rate, 48000);
    assert_eq!(header.version, 1);

    Ok(())
}

#[test]
fn test_ogg_reader_parse_next_page() -> Result<()> {
    let ogg = build_ogg_container();
    let r = Cursor::new(&ogg);
    let (mut reader, _header) = OggReader::new(r, true)?;

    let (payload, _) = reader.parse_next_page()?;
    assert_eq!(payload, Bytes::from_static(&[0x98, 0x36, 0xbe, 0x88, 0x9e]));

    let result = reader.parse_next_page();
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_ogg_reader_parse_errors() -> Result<()> {
    //"Invalid ID Page Header Signature"
    {
        let mut ogg = build_ogg_container();
        ogg[0] = 0;

        let result = OggReader::new(Cursor::new(ogg), false);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err, Error::ErrBadIDPageSignature);
        }
    }

    //"Invalid ID Page Header Type"
    {
        let mut ogg = build_ogg_container();
        ogg[5] = 0;

        let result = OggReader::new(Cursor::new(ogg), false);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err, Error::ErrBadIDPageType);
        }
    }

    //"Invalid ID Page Payload Length"
    {
        let mut ogg = build_ogg_container();
        ogg[27] = 0;

        let result = OggReader::new(Cursor::new(ogg), false);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err, Error::ErrBadIDPageLength);
        }
    }

    //"Invalid ID Page Payload Length"
    {
        let mut ogg = build_ogg_container();
        ogg[35] = 0;

        let result = OggReader::new(Cursor::new(ogg), false);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err, Error::ErrBadIDPagePayloadSignature);
        }
    }

    //"Invalid Page Checksum"
    {
        let mut ogg = build_ogg_container();
        ogg[22] = 0;

        let result = OggReader::new(Cursor::new(ogg), true);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err, Error::ErrChecksumMismatch);
        }
    }

    Ok(())
}
