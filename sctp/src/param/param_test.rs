use super::*;

///////////////////////////////////////////////////////////////////
//param_type_test
///////////////////////////////////////////////////////////////////
use super::param_type::*;

#[test]
fn test_parse_param_type_success() -> Result<()> {
    let tests = vec![
        (Bytes::from_static(&[0x0, 0x1]), ParamType::HeartbeatInfo),
        (Bytes::from_static(&[0x0, 0xd]), ParamType::OutSsnResetReq),
    ];

    for (mut binary, expected) in tests {
        let pt: ParamType = binary.get_u16().into();
        assert_eq!(pt, expected);
    }

    Ok(())
}

///////////////////////////////////////////////////////////////////
//param_header_test
///////////////////////////////////////////////////////////////////
use super::param_header::*;

static PARAM_HEADER_BYTES: Bytes = Bytes::from_static(&[0x0, 0x1, 0x0, 0x4]);

#[test]
fn test_param_header_success() -> Result<()> {
    let tests = vec![(
        PARAM_HEADER_BYTES.clone(),
        ParamHeader {
            typ: ParamType::HeartbeatInfo,
            value_length: 0,
        },
    )];

    for (binary, parsed) in tests {
        let actual = ParamHeader::unmarshal(&binary)?;
        assert_eq!(actual, parsed);
        let b = actual.marshal()?;
        assert_eq!(b, binary);
    }

    Ok(())
}

#[test]
fn test_param_header_unmarshal_failure() -> Result<()> {
    let tests = vec![
        ("header too short", PARAM_HEADER_BYTES.slice(..2)),
        // {"wrong param type", []byte{0x0, 0x0, 0x0, 0x4}}, // Not possible to fail parseParamType atm.
        (
            "reported length below header length",
            Bytes::from_static(&[0x0, 0xd, 0x0, 0x3]),
        ),
        ("wrong reported length", CHUNK_RECONFIG_PARAM_A.slice(0..4)),
    ];

    for (name, binary) in tests {
        let result = ParamHeader::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {name} to fail.");
    }

    Ok(())
}

///////////////////////////////////////////////////////////////////
//param_forward_tsn_supported_test
///////////////////////////////////////////////////////////////////
use super::param_forward_tsn_supported::*;

static PARAM_FORWARD_TSN_SUPPORTED_BYTES: Bytes = Bytes::from_static(&[0xc0, 0x0, 0x0, 0x4]);

#[test]
fn test_param_forward_tsn_supported_success() -> Result<()> {
    let tests = vec![(
        PARAM_FORWARD_TSN_SUPPORTED_BYTES.clone(),
        ParamForwardTsnSupported {},
    )];

    for (binary, parsed) in tests {
        let actual = ParamForwardTsnSupported::unmarshal(&binary)?;
        assert_eq!(actual, parsed);
        let b = actual.marshal()?;
        assert_eq!(b, binary);
    }

    Ok(())
}

#[test]
fn test_param_forward_tsn_supported_failure() -> Result<()> {
    let tests = vec![("param too short", Bytes::from_static(&[0x0, 0xd, 0x0]))];

    for (name, binary) in tests {
        let result = ParamForwardTsnSupported::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {name} to fail.");
    }

    Ok(())
}

///////////////////////////////////////////////////////////////////
//param_outgoing_reset_request_test
///////////////////////////////////////////////////////////////////
use super::param_outgoing_reset_request::*;

static CHUNK_RECONFIG_PARAM_A: Bytes = Bytes::from_static(&[
    0x0, 0xd, 0x0, 0x16, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x2, 0x0, 0x0, 0x0, 0x3, 0x0, 0x4, 0x0,
    0x5, 0x0, 0x6,
]);
static CHUNK_RECONFIG_PARAM_B: Bytes = Bytes::from_static(&[
    0x0, 0xd, 0x0, 0x10, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x2, 0x0, 0x0, 0x0, 0x3,
]);

#[test]
fn test_param_outgoing_reset_request_success() -> Result<()> {
    let tests = vec![
        (
            CHUNK_RECONFIG_PARAM_A.clone(),
            ParamOutgoingResetRequest {
                reconfig_request_sequence_number: 1,
                reconfig_response_sequence_number: 2,
                sender_last_tsn: 3,
                stream_identifiers: vec![4, 5, 6],
            },
        ),
        (
            CHUNK_RECONFIG_PARAM_B.clone(),
            ParamOutgoingResetRequest {
                reconfig_request_sequence_number: 1,
                reconfig_response_sequence_number: 2,
                sender_last_tsn: 3,
                stream_identifiers: vec![],
            },
        ),
    ];

    for (binary, parsed) in tests {
        let actual = ParamOutgoingResetRequest::unmarshal(&binary)?;
        assert_eq!(actual, parsed);
        let b = actual.marshal()?;
        assert_eq!(b, binary);
    }

    Ok(())
}

#[test]
fn test_param_outgoing_reset_request_failure() -> Result<()> {
    let tests = vec![
        ("packet too short", CHUNK_RECONFIG_PARAM_A.slice(..8)),
        ("param too short", Bytes::from_static(&[0x0, 0xd, 0x0, 0x4])),
    ];

    for (name, binary) in tests {
        let result = ParamOutgoingResetRequest::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {name} to fail.");
    }

    Ok(())
}

///////////////////////////////////////////////////////////////////
//param_reconfig_response_test
///////////////////////////////////////////////////////////////////
use super::param_reconfig_response::*;
use bytes::Buf;

static CHUNK_RECONFIG_RESPONCE: Bytes =
    Bytes::from_static(&[0x0, 0x10, 0x0, 0xc, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x1]);

#[test]
fn test_param_reconfig_response_success() -> Result<()> {
    let tests = vec![(
        CHUNK_RECONFIG_RESPONCE.clone(),
        ParamReconfigResponse {
            reconfig_response_sequence_number: 1,
            result: ReconfigResult::SuccessPerformed,
        },
    )];

    for (binary, parsed) in tests {
        let actual = ParamReconfigResponse::unmarshal(&binary)?;
        assert_eq!(actual, parsed);
        let b = actual.marshal()?;
        assert_eq!(b, binary);
    }

    Ok(())
}

#[test]
fn test_param_reconfig_response_failure() -> Result<()> {
    let tests = vec![
        ("packet too short", CHUNK_RECONFIG_RESPONCE.slice(..8)),
        (
            "param too short",
            Bytes::from_static(&[0x0, 0x10, 0x0, 0x4]),
        ),
    ];

    for (name, binary) in tests {
        let result = ParamReconfigResponse::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {name} to fail.");
    }

    Ok(())
}

#[test]
fn test_reconfig_result_stringer() -> Result<()> {
    let tests = vec![
        (ReconfigResult::SuccessNop, "0: Success - Nothing to do"),
        (ReconfigResult::SuccessPerformed, "1: Success - Performed"),
        (ReconfigResult::Denied, "2: Denied"),
        (ReconfigResult::ErrorWrongSsn, "3: Error - Wrong SSN"),
        (
            ReconfigResult::ErrorRequestAlreadyInProgress,
            "4: Error - Request already in progress",
        ),
        (
            ReconfigResult::ErrorBadSequenceNumber,
            "5: Error - Bad Sequence Number",
        ),
        (ReconfigResult::InProgress, "6: In progress"),
    ];

    for (result, expected) in tests {
        let actual = result.to_string();
        assert_eq!(actual, expected, "Test case {expected}");
    }

    Ok(())
}

///////////////////////////////////////////////////////////////////
//param_test
///////////////////////////////////////////////////////////////////

#[test]
fn test_build_param_success() -> Result<()> {
    let tests = vec![CHUNK_RECONFIG_PARAM_A.clone()];

    for binary in tests {
        let p = build_param(&binary)?;
        let b = p.marshal()?;
        assert_eq!(b, binary);
    }

    Ok(())
}

#[test]
fn test_build_param_failure() -> Result<()> {
    let tests = vec![
        ("invalid ParamType", Bytes::from_static(&[0x0, 0x0])),
        ("build failure", CHUNK_RECONFIG_PARAM_A.slice(..8)),
    ];

    for (name, binary) in tests {
        let result = build_param(&binary);
        assert!(result.is_err(), "expected unmarshal: {name} to fail.");
    }

    Ok(())
}
