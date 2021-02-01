#[cfg(test)]
mod tests {
    use crate::errors::ExtensionError;
    use crate::extension::transport_cc_extension::*;

    #[test]
    fn test_transport_cc_extension_too_small() -> Result<(), ExtensionError> {
        let mut t1 = TransportCCExtension::default();

        let result = t1.unmarshal(&mut BytesMut::new());

        assert_eq!(
            result.err(),
            Some(ExtensionError::TooSmall),
            "err != errTooSmall"
        );

        Ok(())
    }

    #[test]
    fn test_transport_cc_extension() -> Result<(), ExtensionError> {
        let raw: Vec<u8> = vec![0x00, 0x02];

        let mut t1 = TransportCCExtension::default();

        t1.unmarshal(&mut raw[..].into())?;

        let t2 = TransportCCExtension {
            transport_sequence: 2,
        };

        assert_eq!(t1, t2);

        let dst_data = t2.marshal()?;

        assert_eq!(raw, dst_data, "Marshal failed");

        Ok(())
    }

    #[test]
    fn test_transport_cc_extension_extra_bytes() -> Result<(), ExtensionError> {
        let raw: Vec<u8> = vec![0x00, 0x02, 0x00, 0xff, 0xff];

        let mut t1 = TransportCCExtension::default();

        t1.unmarshal(&mut raw[..].into())?;

        let t2 = TransportCCExtension {
            transport_sequence: 2,
        };

        assert_eq!(t1, t2);

        Ok(())
    }
}
