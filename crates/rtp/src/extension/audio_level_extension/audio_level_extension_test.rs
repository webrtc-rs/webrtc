use super::*;

use std::io::{BufReader, BufWriter};

#[test]
fn test_audio_level_extension_too_small() -> Result<(), Error> {
    let raw: Vec<u8> = vec![];
    let mut reader = BufReader::new(raw.as_slice());
    let result = AudioLevelExtension::unmarshal(&mut reader);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_audio_level_extension_voice_true() -> Result<(), Error> {
    let raw: Vec<u8> = vec![0x88];
    let mut reader = BufReader::new(raw.as_slice());
    let a1 = AudioLevelExtension::unmarshal(&mut reader)?;
    let a2 = AudioLevelExtension {
        level: 8,
        voice: true,
    };
    assert_eq!(a1, a2);

    let mut dst: Vec<u8> = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(dst.as_mut());
        a2.marshal(&mut writer)?;
    }
    assert_eq!(raw, dst);

    Ok(())
}

#[test]
fn test_audio_level_extension_voice_false() -> Result<(), Error> {
    let raw: Vec<u8> = vec![0x8];
    let mut reader = BufReader::new(raw.as_slice());
    let a1 = AudioLevelExtension::unmarshal(&mut reader)?;
    let a2 = AudioLevelExtension {
        level: 8,
        voice: false,
    };
    assert_eq!(a1, a2);

    let mut dst: Vec<u8> = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(dst.as_mut());
        a2.marshal(&mut writer)?;
    }
    assert_eq!(raw, dst);

    Ok(())
}

#[test]
fn test_audio_level_extension_level_overflow() -> Result<(), Error> {
    let a = AudioLevelExtension {
        level: 128,
        voice: false,
    };

    let mut dst: Vec<u8> = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(dst.as_mut());
        let result = a.marshal(&mut writer);
        assert!(result.is_err());
    }

    Ok(())
}
