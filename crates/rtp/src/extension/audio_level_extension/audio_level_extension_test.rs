// #[cfg(test)]
// mod tests {
//     use crate::extension::audio_level_extension::*;

//     #[test]
//     fn test_audio_level_extension_too_small() {
//         let mut a = AudioLevelExtension::default();

//         let result = a.unmarshal(&mut BytesMut::new());
//         assert_eq!(
//             result.err(),
//             Some(ExtensionError::TooSmall),
//             "err != errTooSmall"
//         );
//     }

//     #[test]
//     fn test_audio_level_extension_voice_true() -> Result<(), ExtensionError> {
//         let raw: Vec<u8> = vec![0x88];

//         let mut a1 = AudioLevelExtension::default();

//         a1.unmarshal(&mut raw[..].into())?;
//         let a2 = AudioLevelExtension {
//             level: 8,
//             voice: true,
//         };

//         assert_eq!(a1, a2);

//         let dst = a2.marshal()?;
//         assert_eq!(raw, dst, "Marshal failed");

//         Ok(())
//     }

//     #[test]
//     fn test_audio_level_extension_voice_false() -> Result<(), ExtensionError> {
//         let raw: Vec<u8> = vec![0x8];
//         let mut a1 = AudioLevelExtension::default();

//         a1.unmarshal(&mut raw[..].into())?;

//         let a2 = AudioLevelExtension {
//             level: 8,
//             voice: false,
//         };

//         assert_eq!(a1, a2, "unmarshal failed");

//         let dst_data = a2.marshal()?;
//         assert_eq!(raw, dst_data);

//         Ok(())
//     }

//     #[test]
//     fn test_audio_level_extension_level_overflow() -> Result<(), ExtensionError> {
//         let a = AudioLevelExtension {
//             level: 128,
//             voice: false,
//         };

//         let result = a.marshal();
//         assert_eq!(
//             result.err(),
//             Some(ExtensionError::AudioLevelOverflow),
//             "err != errAudioOverflow"
//         );

//         Ok(())
//     }
// }
