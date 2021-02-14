// #[cfg(test)]
// mod tests {
//     use crate::codecs::opus::*;

//     #[test]
//     fn test_opus_unmarshal() -> Result<(), RTPError> {
//         let mut pck = OpusPacket::default();

//         // Empty packet
//         let result = pck.unmarshal(&mut [][..].into());
//         assert!(result.is_err(), "Result should be err in case of error");

//         // Normal packet
//         let raw_bytes = vec![0x00u8, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x90];
//         let raw = pck.unmarshal(&mut raw_bytes[..].into())?;
//         assert!(!raw.is_empty(), "Payload must be same");

//         Ok(())
//     }

//     #[test]
//     fn test_opus_payload() {
//         let pck = OpusPayloader;
//         let payload = vec![0x90, 0x90, 0x90];

//         // Positive MTU, empty payload
//         let result = pck.payload(1, BytesMut::new());
//         assert!(result.is_empty(), "Generated payload should be empty");

//         // Positive MTU, small payload
//         let result = pck.payload(1, payload[..].into());
//         assert_eq!(result.len(), 1, "Generated payload should be the 1");

//         // Positive MTU, small payload
//         let result = pck.payload(2, payload[..].into());
//         assert_eq!(result.len(), 1, "Generated payload should be the 1");
//     }
// }
