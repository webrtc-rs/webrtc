use super::*;

#[test]
fn test_audio_level_extension_too_small() -> Result<(), Error> {
    let raw = Bytes::from_static(&[]);
    let result = AudioLevelExtension::unmarshal(&raw);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_audio_level_extension_voice_true() -> Result<(), Error> {
    let raw = Bytes::from_static(&[0x88]);
    let a1 = AudioLevelExtension::unmarshal(&raw)?;
    let a2 = AudioLevelExtension {
        level: 8,
        voice: true,
    };
    assert_eq!(a1, a2);

    let mut dst = BytesMut::with_capacity(a2.marshal_size());
    a2.marshal_to(&mut dst)?;
    assert_eq!(raw, dst.freeze());

    Ok(())
}

#[test]
fn test_audio_level_extension_voice_false() -> Result<(), Error> {
    let raw = Bytes::from_static(&[0x8]);
    let a1 = AudioLevelExtension::unmarshal(&raw)?;
    let a2 = AudioLevelExtension {
        level: 8,
        voice: false,
    };
    assert_eq!(a1, a2);

    let mut dst = BytesMut::with_capacity(a2.marshal_size());
    a2.marshal_to(&mut dst)?;
    assert_eq!(raw, dst.freeze());

    Ok(())
}

#[test]
fn test_audio_level_extension_level_overflow() -> Result<(), Error> {
    let a = AudioLevelExtension {
        level: 128,
        voice: false,
    };

    let mut dst = BytesMut::with_capacity(a.marshal_size());
    let result = a.marshal_to(&mut dst);
    assert!(result.is_err());

    Ok(())
}
