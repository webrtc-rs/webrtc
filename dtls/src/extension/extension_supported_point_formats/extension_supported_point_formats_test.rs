use std::io::{BufReader, BufWriter};

use super::*;

#[test]
fn test_extension_supported_point_formats() -> Result<()> {
    let raw_extension_supported_point_formats = vec![0x00, 0x02, 0x01, 0x00]; // 0x00, 0x0b,
    let parsed_extension_supported_point_formats = ExtensionSupportedPointFormats {
        point_formats: vec![ELLIPTIC_CURVE_POINT_FORMAT_UNCOMPRESSED],
    };

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        parsed_extension_supported_point_formats.marshal(&mut writer)?;
    }

    assert_eq!(
        raw, raw_extension_supported_point_formats,
        "extensionSupportedPointFormats marshal: got {raw:?}, want {raw_extension_supported_point_formats:?}"
    );

    let mut reader = BufReader::new(raw.as_slice());
    let new_extension_supported_point_formats =
        ExtensionSupportedPointFormats::unmarshal(&mut reader)?;

    assert_eq!(
        new_extension_supported_point_formats, parsed_extension_supported_point_formats,
        "extensionSupportedPointFormats unmarshal: got {new_extension_supported_point_formats:?}, want {parsed_extension_supported_point_formats:?}"
    );

    Ok(())
}
