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
