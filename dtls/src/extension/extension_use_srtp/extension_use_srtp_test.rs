use std::io::{BufReader, BufWriter};

use super::*;

#[test]
fn test_extension_use_srtp() -> Result<()> {
    let raw_use_srtp = vec![0x00, 0x05, 0x00, 0x02, 0x00, 0x01, 0x00]; //0x00, 0x0e,
    let parsed_use_srtp = ExtensionUseSrtp {
        protection_profiles: vec![SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80],
    };

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        parsed_use_srtp.marshal(&mut writer)?;
    }

    assert_eq!(
        raw, raw_use_srtp,
        "extensionUseSRTP marshal: got {raw:?}, want {raw_use_srtp:?}"
    );

    let mut reader = BufReader::new(raw.as_slice());
    let new_use_srtp = ExtensionUseSrtp::unmarshal(&mut reader)?;

    assert_eq!(
        new_use_srtp, parsed_use_srtp,
        "extensionUseSRTP unmarshal: got {new_use_srtp:?}, want {parsed_use_srtp:?}"
    );

    Ok(())
}
