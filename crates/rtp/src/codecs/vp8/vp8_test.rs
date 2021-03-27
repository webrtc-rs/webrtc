#[cfg(test)]
mod tests {
    use crate::codecs::vp8::*;

    #[test]
    fn test_vp8_unmarshal() -> Result<(), RTPError> {
        let mut pck = VP8Packet::default();

        // Empty packet
        let result = pck.unmarshal(&mut []);
        assert_eq!(
            result.err(),
            Some(RTPError::ShortPacket),
            "Result should be err in case of error"
        );

        // Payload smaller than header size
        let small_bytes = &mut [0x00, 0x11, 0x22];
        let result = pck.unmarshal(small_bytes);
        assert_eq!(
            result.err(),
            Some(RTPError::ShortPacket),
            "Result should be err in case of error"
        );

        // Payload smaller than header size
        let small_bytes = &mut [0x00u8, 0x11];
        let result = pck.unmarshal(small_bytes);
        assert_eq!(
            result.err(),
            Some(RTPError::ShortPacket),
            "Result should be err in case of error",
        );

        // Normal packet
        let raw_bytes = &mut [0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x90];
        let result = pck.unmarshal(raw_bytes)?;
        assert!(!result.is_empty(), "Payload must be not empty");

        // Header size, only X
        let raw_bytes = &mut [0x80, 0x00, 0x00, 0x00];
        let result = pck.unmarshal(raw_bytes)?;
        assert!(!result.is_empty(), "Payload must be not empty");

        // Header size, only X and I
        let raw_bytes = &mut [0x80, 0x80, 0x00, 0x00];
        let result = pck.unmarshal(raw_bytes)?;
        assert!(!result.is_empty(), "Payload must be not empty");

        // Header size, X and I, PID 16bits
        let raw_bytes = &mut [0x80, 0x80, 0x81, 0x00, 0x00];
        let result = pck.unmarshal(raw_bytes)?;
        assert!(!result.is_empty(), "Payload must be not empty");

        // Header size, X and L
        let raw_bytes = &mut [0x80, 0x40, 0x00, 0x00];
        let result = pck.unmarshal(raw_bytes)?;
        assert!(!result.is_empty(), "Payload must be not empty");

        // Header size, X and T
        let raw_bytes = &mut [0x80, 0x20, 0x00, 0x00];
        let result = pck.unmarshal(raw_bytes)?;
        assert!(!result.is_empty(), "Payload must be not empty");

        // Header size, X and K
        let raw_bytes = &mut [0x80, 0x10, 0x00, 0x00];
        let result = pck.unmarshal(raw_bytes)?;
        assert!(!result.is_empty(), "Payload must be not empty");

        // Header size, all flags and 16bit picture_id
        let raw_bytes = &mut [0xff, 0xff, 0x00, 0x00];
        let result = pck.unmarshal(raw_bytes);
        assert_eq!(
            result.err(),
            Some(RTPError::ShortPacket),
            "Payload must be not empty"
        );

        Ok(())
    }

    #[test]
    fn test_vp8_payload() {
        let pck = VP8Payloader;
        let payload = vec![0x90, 0x90, 0x90];

        // Positive MTU, empty payload
        let result = pck.payload(1, &[]);
        assert!(result.is_empty(), "Generated payload should be empty");

        // Positive MTU, small payload
        let result = pck.payload(1, payload[..].into());
        assert!(result.is_empty(), "Generated payload should be empty");

        // Positive MTU, small payload
        let result = pck.payload(2, payload[..].into());
        assert_eq!(
            result.len(),
            payload.len(),
            "Generated payload should be the same size as original payload size"
        );
    }

    #[test]
    fn test_vp8_partition_head_checker_is_partitioned() {
        let mut checker = VP8PartitionHeadChecker;

        assert!(
            !checker.is_partition_head(&mut [0x00]),
            "Small packet should not be the head of a new partition"
        );

        assert!(
            checker.is_partition_head(&mut [0x10, 0x00, 0x00, 0x00]),
            "Packet with S flag should be the head of a new partition"
        );

        assert!(
            !checker.is_partition_head(&mut [0x00, 0x00, 0x00, 0x00][..]),
            "Packet without S flag should not be the head of a new partition"
        );
    }
}
