use super::*;

///////////////////////////////////////////////////////////////////
//param_forward_tsn_supported_test
///////////////////////////////////////////////////////////////////
use super::param_forward_tsn_supported::*;

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
fn test_param_outgoing_reset_request_success() -> Result<(), Error> {
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
        assert_eq!(parsed, actual);
        let b = actual.marshal()?;
        assert_eq!(binary, b);
    }

    Ok(())
}

#[test]
fn test_param_outgoing_reset_request_failure() -> Result<(), Error> {
    let tests = vec![
        ("packet too short", CHUNK_RECONFIG_PARAM_A.slice(..8)),
        ("param too short", Bytes::from_static(&[0x0, 0xd, 0x0, 0x4])),
    ];

    for (name, binary) in tests {
        let result = ParamOutgoingResetRequest::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {} to fail.", name);
    }

    Ok(())
}

///////////////////////////////////////////////////////////////////
//param_reconfig_response_test
///////////////////////////////////////////////////////////////////
use super::param_reconfig_response::*;

static CHUNK_RECONFIG_RESPONCE: Bytes =
    Bytes::from_static(&[0x0, 0x10, 0x0, 0xc, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x1]);

#[test]
fn test_param_reconfig_response_success() -> Result<(), Error> {
    let tests = vec![(
        CHUNK_RECONFIG_RESPONCE.clone(),
        ParamReconfigResponse {
            reconfig_response_sequence_number: 1,
            result: ReconfigResult::SuccessPerformed,
        },
    )];

    for (binary, parsed) in tests {
        let actual = ParamReconfigResponse::unmarshal(&binary)?;
        assert_eq!(parsed, actual);
        let b = actual.marshal()?;
        assert_eq!(binary, b);
    }

    Ok(())
}

#[test]
fn test_param_reconfig_response_failure() -> Result<(), Error> {
    let tests = vec![
        ("packet too short", CHUNK_RECONFIG_RESPONCE.slice(..8)),
        (
            "param too short",
            Bytes::from_static(&[0x0, 0x10, 0x0, 0x4]),
        ),
    ];

    for (name, binary) in tests {
        let result = ParamReconfigResponse::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {} to fail.", name);
    }

    Ok(())
}

#[test]
fn test_reconfig_result_stringer() -> Result<(), Error> {
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
        assert_eq!(expected, actual, "Test case {}", expected);
    }

    Ok(())
}
