#[cfg(test)]
mod tests {
    use crate::errors::ExtensionError;
    use crate::extension::transport_cc_extension::*;

    #[test]
    fn test_transport_cc_extension_too_small() -> Result<(), ExtensionError> {
        let mut t1 = TransportCCExtension::default();

        let result = t1.unmarshal(&mut []);

        assert_eq!(
            result.err(),
            Some(ExtensionError::TooSmall),
            "err != errTooSmall"
        );

        Ok(())
    }

    #[test]
    fn test_transport_cc_extension() -> Result<(), ExtensionError> {
        let raw = &mut [0x00, 0x02];

        let mut t1 = TransportCCExtension::default();

        t1.unmarshal(raw)?;

        let t2 = TransportCCExtension {
            transport_sequence: 2,
        };

        assert_eq!(t1, t2);

        let mut dst_data = t2.marshal()?;

        assert_eq!(raw, dst_data.as_mut_slice(), "Marshal failed");

        Ok(())
    }

    #[test]
    fn test_transport_cc_extension_extra_bytes() -> Result<(), ExtensionError> {
        let raw = &mut [0x00, 0x02, 0x00, 0xff, 0xff];

        let mut t1 = TransportCCExtension::default();

        t1.unmarshal(raw)?;

        let t2 = TransportCCExtension {
            transport_sequence: 2,
        };

        assert_eq!(t1, t2);

        Ok(())
    }
}
