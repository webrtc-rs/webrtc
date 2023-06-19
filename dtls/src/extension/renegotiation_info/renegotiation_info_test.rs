use std::io::{BufReader, BufWriter};

use super::*;

#[test]
fn test_renegotiation_info() -> Result<()> {
    let extension = ExtensionRenegotiationInfo {
        renegotiated_connection: 0,
    };

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        extension.marshal(&mut writer)?;
    }

    let mut reader = BufReader::new(raw.as_slice());
    let new_extension = ExtensionRenegotiationInfo::unmarshal(&mut reader)?;

    assert_eq!(
        new_extension.renegotiated_connection,
        extension.renegotiated_connection
    );

    Ok(())
}
