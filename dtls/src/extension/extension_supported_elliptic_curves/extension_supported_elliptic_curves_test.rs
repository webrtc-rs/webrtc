use super::*;

use std::io::{BufReader, BufWriter};

#[test]
fn test_extension_supported_groups() -> Result<()> {
    let raw_supported_groups = vec![0x0, 0x4, 0x0, 0x2, 0x0, 0x1d]; // 0x0, 0xa,
    let parsed_supported_groups = ExtensionSupportedEllipticCurves {
        elliptic_curves: vec![NamedCurve::X25519],
    };

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        parsed_supported_groups.marshal(&mut writer)?;
    }

    assert_eq!(
        raw, raw_supported_groups,
        "extensionSupportedGroups marshal: got {:?}, want {:?}",
        raw, raw_supported_groups
    );

    let mut reader = BufReader::new(raw.as_slice());
    let new_supported_groups = ExtensionSupportedEllipticCurves::unmarshal(&mut reader)?;

    assert_eq!(
        new_supported_groups, parsed_supported_groups,
        "extensionSupportedGroups unmarshal: got {:?}, want {:?}",
        new_supported_groups, parsed_supported_groups
    );

    Ok(())
}
