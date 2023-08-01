use bytes::{Bytes, BytesMut};

use super::*;
use crate::error::Result;

#[test]
fn test_audio_level_extension_too_small() -> Result<()> {
    let mut buf = &vec![0u8; 0][..];
    let result = AudioLevelExtension::unmarshal(&mut buf);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_audio_level_extension_voice_true() -> Result<()> {
    let raw = Bytes::from_static(&[0x88]);
    let buf = &mut raw.clone();
    let a1 = AudioLevelExtension::unmarshal(buf)?;
    let a2 = AudioLevelExtension {
        level: 8,
        voice: true,
    };
    assert_eq!(a1, a2);

    let mut dst = BytesMut::with_capacity(a2.marshal_size());
    dst.resize(a2.marshal_size(), 0);
    a2.marshal_to(&mut dst)?;
    assert_eq!(raw, dst.freeze());

    Ok(())
}

#[test]
fn test_audio_level_extension_voice_false() -> Result<()> {
    let raw = Bytes::from_static(&[0x8]);
    let buf = &mut raw.clone();
    let a1 = AudioLevelExtension::unmarshal(buf)?;
    let a2 = AudioLevelExtension {
        level: 8,
        voice: false,
    };
    assert_eq!(a1, a2);

    let mut dst = BytesMut::with_capacity(a2.marshal_size());
    dst.resize(a2.marshal_size(), 0);
    a2.marshal_to(&mut dst)?;
    assert_eq!(raw, dst.freeze());

    Ok(())
}

#[test]
fn test_audio_level_extension_level_overflow() -> Result<()> {
    let a = AudioLevelExtension {
        level: 128,
        voice: false,
    };

    let mut dst = BytesMut::with_capacity(a.marshal_size());
    dst.resize(a.marshal_size(), 0);
    let result = a.marshal_to(&mut dst);
    assert!(result.is_err());

    Ok(())
}
