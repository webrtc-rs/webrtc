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
