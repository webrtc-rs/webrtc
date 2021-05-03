use super::*;

///////////////////////////////////////////////////////////////////
//chunk_abort_test
///////////////////////////////////////////////////////////////////
use super::chunk_abort::*;
use crate::error_cause::*;

#[test]
fn test_abort_chunk_one_error_cause() -> Result<(), Error> {
    let abort1 = ChunkAbort {
        error_causes: vec![ErrorCause {
            code: PROTOCOL_VIOLATION,
            ..Default::default()
        }],
    };

    let b = abort1.marshal()?;
    let abort2 = ChunkAbort::unmarshal(&b)?;

    assert_eq!(1, abort2.error_causes.len(), "should have only one cause");
    assert_eq!(
        abort1.error_causes[0].error_cause_code(),
        abort2.error_causes[0].error_cause_code(),
        "errorCause code should match"
    );

    Ok(())
}

#[test]
fn test_abort_chunk_many_error_causes() -> Result<(), Error> {
    let abort1 = ChunkAbort {
        error_causes: vec![
            ErrorCause {
                code: INVALID_MANDATORY_PARAMETER,
                ..Default::default()
            },
            ErrorCause {
                code: UNRECOGNIZED_CHUNK_TYPE,
                ..Default::default()
            },
            ErrorCause {
                code: PROTOCOL_VIOLATION,
                ..Default::default()
            },
        ],
    };

    let b = abort1.marshal()?;
    let abort2 = ChunkAbort::unmarshal(&b)?;
    assert_eq!(3, abort2.error_causes.len(), "should have only one cause");
    for (i, error_cause) in abort1.error_causes.iter().enumerate() {
        assert_eq!(
            error_cause.error_cause_code(),
            abort2.error_causes[i].error_cause_code(),
            "errorCause code should match"
        );
    }

    Ok(())
}

///////////////////////////////////////////////////////////////////
//chunk_error_test
///////////////////////////////////////////////////////////////////
use super::chunk_error::*;
use super::chunk_type::*;
use bytes::BufMut;
use lazy_static::lazy_static;

const CHUNK_FLAGS: u8 = 0x00;
static ORG_UNRECOGNIZED_CHUNK: Bytes =
    Bytes::from_static(&[0xc0, 0x0, 0x0, 0x8, 0x0, 0x0, 0x0, 0x3]);

lazy_static! {
    static ref RAW_IN: Bytes = {
        let mut raw = BytesMut::new();
        raw.put_u8(ChunkType::Error as u8);
        raw.put_u8(CHUNK_FLAGS);
        raw.extend(vec![0x00, 0x10, 0x00, 0x06, 0x00, 0x0c]);
        raw.extend(ORG_UNRECOGNIZED_CHUNK.clone());
        raw.freeze()
    };
}

#[test]
fn test_chunk_error_unrecognized_chunk_type_unmarshal() -> Result<(), Error> {
    let c = ChunkError::unmarshal(&RAW_IN)?;
    assert_eq!(
        ChunkType::Error,
        c.header().typ,
        "chunk type should be ERROR"
    );
    assert_eq!(1, c.error_causes.len(), "there should be on errorCause");

    let ec = &c.error_causes[0];
    assert_eq!(
        UNRECOGNIZED_CHUNK_TYPE,
        ec.error_cause_code(),
        "cause code should be unrecognizedChunkType"
    );
    assert_eq!(
        ec.raw, ORG_UNRECOGNIZED_CHUNK,
        "should have valid unrecognizedChunk"
    );

    Ok(())
}

#[test]
fn test_chunk_error_unrecognized_chunk_type_marshal() -> Result<(), Error> {
    let ec_unrecognized_chunk_type = ErrorCause {
        code: UNRECOGNIZED_CHUNK_TYPE,
        raw: ORG_UNRECOGNIZED_CHUNK.clone(),
    };

    let ec = ChunkError {
        error_causes: vec![ec_unrecognized_chunk_type],
    };

    let raw = ec.marshal()?;
    assert_eq!(raw, *RAW_IN, "unexpected serialization result");

    Ok(())
}

#[test]
fn test_chunk_error_unrecognized_chunk_type_marshal_with_cause_value_being_nil() -> Result<(), Error>
{
    let expected = Bytes::from_static(&[
        ChunkType::Error as u8,
        CHUNK_FLAGS,
        0x00,
        0x08,
        0x00,
        0x06,
        0x00,
        0x04,
    ]);
    let ec_unrecognized_chunk_type = ErrorCause {
        code: UNRECOGNIZED_CHUNK_TYPE,
        ..Default::default()
    };

    let ec = ChunkError {
        error_causes: vec![ec_unrecognized_chunk_type],
    };

    let raw = ec.marshal()?;
    assert_eq!(raw, expected, "unexpected serialization result");

    Ok(())
}

///////////////////////////////////////////////////////////////////
//chunk_forward_tsn_test
///////////////////////////////////////////////////////////////////
use super::chunk_forward_tsn::*;

static CHUNK_FORWARD_TSN_BYTES: Bytes =
    Bytes::from_static(&[0xc0, 0x0, 0x0, 0x8, 0x0, 0x0, 0x0, 0x3]);

#[test]
fn test_chunk_forward_tsn_success() -> Result<(), Error> {
    let tests = vec![
        CHUNK_FORWARD_TSN_BYTES.clone(),
        Bytes::from_static(&[0xc0, 0x0, 0x0, 0xc, 0x0, 0x0, 0x0, 0x3, 0x0, 0x4, 0x0, 0x5]),
        Bytes::from_static(&[
            0xc0, 0x0, 0x0, 0x10, 0x0, 0x0, 0x0, 0x3, 0x0, 0x4, 0x0, 0x5, 0x0, 0x6, 0x0, 0x7,
        ]),
    ];

    for binary in tests {
        let actual = ChunkForwardTsn::unmarshal(&binary)?;
        let b = actual.marshal()?;
        assert_eq!(binary, b, "test not equal");
    }

    Ok(())
}

#[test]
fn test_chunk_forward_tsn_unmarshal_failure() -> Result<(), Error> {
    let tests = vec![
        ("chunk header to short", Bytes::from_static(&[0xc0])),
        (
            "missing New Cumulative TSN",
            Bytes::from_static(&[0xc0, 0x0, 0x0, 0x4]),
        ),
        (
            "missing stream sequence",
            Bytes::from_static(&[
                0xc0, 0x0, 0x0, 0xe, 0x0, 0x0, 0x0, 0x3, 0x0, 0x4, 0x0, 0x5, 0x0, 0x6,
            ]),
        ),
    ];

    for (name, binary) in tests {
        let result = ChunkForwardTsn::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {} to fail.", name);
    }

    Ok(())
}

///////////////////////////////////////////////////////////////////
//chunk_reconfig_test
///////////////////////////////////////////////////////////////////
use super::chunk_reconfig::*;

static TEST_CHUNK_RECONFIG_PARAM_A: Bytes = Bytes::from_static(&[
    0x0, 0xd, 0x0, 0x16, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x2, 0x0, 0x0, 0x0, 0x3, 0x0, 0x4, 0x0,
    0x5, 0x0, 0x6,
]);

static TEST_CHUNK_RECONFIG_PARAM_B: Bytes = Bytes::from_static(&[
    0x0, 0xd, 0x0, 0x10, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x2, 0x0, 0x0, 0x0, 0x3,
]);

static TEST_CHUNK_RECONFIG_RESPONCE: Bytes =
    Bytes::from_static(&[0x0, 0x10, 0x0, 0xc, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x1]);

lazy_static! {
    static ref TEST_CHUNK_RECONFIG_BYTES: Vec<Bytes> = {
        let mut tests = vec![];
        {
            let mut test = BytesMut::new();
            test.extend(vec![0x82, 0x0, 0x0, 0x1a]);
            test.extend(TEST_CHUNK_RECONFIG_PARAM_A.clone());
            tests.push(test.freeze());
        }
        {
            let mut test = BytesMut::new();
            test.extend(vec![0x82, 0x0, 0x0, 0x14]);
            test.extend(TEST_CHUNK_RECONFIG_PARAM_B.clone());
            tests.push(test.freeze());
        }
        {
            let mut test = BytesMut::new();
            test.extend(vec![0x82, 0x0, 0x0, 0x10]);
            test.extend(TEST_CHUNK_RECONFIG_RESPONCE.clone());
            tests.push(test.freeze());
        }
        {
            let mut test = BytesMut::new();
            test.extend(vec![0x82, 0x0, 0x0, 0x2c]);
            test.extend(TEST_CHUNK_RECONFIG_PARAM_A.clone());
            test.extend(vec![0u8; 2]);
            test.extend(TEST_CHUNK_RECONFIG_PARAM_B.clone());
            tests.push(test.freeze());
        }
        {
            let mut test = BytesMut::new();
            test.extend(vec![0x82, 0x0, 0x0, 0x2a]);
            test.extend(TEST_CHUNK_RECONFIG_PARAM_B.clone());
            test.extend(TEST_CHUNK_RECONFIG_PARAM_A.clone());
            tests.push(test.freeze());
        }

        tests
    };
}

#[test]
fn test_chunk_reconfig_success() -> Result<(), Error> {
    for (i, binary) in TEST_CHUNK_RECONFIG_BYTES.iter().enumerate() {
        let actual = ChunkReconfig::unmarshal(binary)?;
        let b = actual.marshal()?;
        assert_eq!(*binary, b, "test {} not equal: {:?} vs {:?}", i, *binary, b);
    }

    Ok(())
}

#[test]
fn test_chunk_reconfig_unmarshal_failure() -> Result<(), Error> {
    let mut test = BytesMut::new();
    test.extend(vec![0x82, 0x0, 0x0, 0x18]);
    test.extend(TEST_CHUNK_RECONFIG_PARAM_B.clone());
    test.extend(vec![0x0, 0xd, 0x0, 0x0]);
    let tests = vec![
        ("chunk header to short", Bytes::from_static(&[0x82])),
        (
            "missing parse param type (A)",
            Bytes::from_static(&[0x82, 0x0, 0x0, 0x4]),
        ),
        (
            "wrong param (A)",
            Bytes::from_static(&[0x82, 0x0, 0x0, 0x8, 0x0, 0xd, 0x0, 0x0]),
        ),
        ("wrong param (B)", test.freeze()),
    ];

    for (name, binary) in tests {
        let result = ChunkReconfig::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {} to fail.", name);
    }

    Ok(())
}
