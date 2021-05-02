use super::{param_forward_tsn_supported::*, *};

///////////////////////////////////////////////////////////////////
//param_forward_tsn_supported_test
///////////////////////////////////////////////////////////////////

static PARAM_FORWARD_TSN_SUPPORTED_BYTES: Bytes = Bytes::from_static(&[0xc0, 0x0, 0x0, 0x4]);

#[test]
fn test_param_forward_tsn_supported_success() -> Result<(), Error> {
    let tests = vec![(
        PARAM_FORWARD_TSN_SUPPORTED_BYTES.clone(),
        ParamForwardTsnSupported {},
    )];

    for (binary, parsed) in tests {
        let actual = ParamForwardTsnSupported::unmarshal(&binary)?;
        assert_eq!(parsed, actual);
        let b = actual.marshal()?;
        assert_eq!(binary, b);
    }

    Ok(())
}

#[test]
fn test_param_forward_tsn_supported_failure() -> Result<(), Error> {
    let tests = vec![("param too short", Bytes::from_static(&[0x0, 0xd, 0x0]))];

    for (name, binary) in tests {
        let result = ParamForwardTsnSupported::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {} to fail.", name);
    }

    Ok(())
}
