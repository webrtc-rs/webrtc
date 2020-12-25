use super::*;

use std::io::{BufReader, BufWriter};

use util::Error;

#[test]
fn test_extension_server_name() -> Result<(), Error> {
    let extension = ExtensionServerName {
        server_name: "test.domain".to_owned(),
    };

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        extension.marshal(&mut writer)?;
    }

    let mut reader = BufReader::new(raw.as_slice());
    let new_extension = ExtensionServerName::unmarshal(&mut reader)?;

    assert_eq!(
        new_extension, extension,
        "extensionServerName marshal: got {:?} expected {:?}",
        new_extension, extension,
    );

    Ok(())
}
