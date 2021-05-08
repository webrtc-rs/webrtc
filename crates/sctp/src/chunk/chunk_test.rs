use super::*;

///////////////////////////////////////////////////////////////////
//chunk_type_test
///////////////////////////////////////////////////////////////////
use super::chunk_type::*;

#[test]
fn test_chunk_type_string() -> Result<(), Error> {
    let tests = vec![
        (ChunkType::PayloadData, "DATA"),
        (ChunkType::Init, "INIT"),
        (ChunkType::InitAck, "INIT-ACK"),
        (ChunkType::Sack, "SACK"),
        (ChunkType::Heartbeat, "HEARTBEAT"),
        (ChunkType::HeartbeatAck, "HEARTBEAT-ACK"),
        (ChunkType::Abort, "ABORT"),
        (ChunkType::Shutdown, "SHUTDOWN"),
        (ChunkType::ShutdownAck, "SHUTDOWN-ACK"),
        (ChunkType::Error, "ERROR"),
        (ChunkType::CookieEcho, "COOKIE-ECHO"),
        (ChunkType::CookieAck, "COOKIE-ACK"),
        (ChunkType::Cwr, "ECNE"),
        (ChunkType::ShutdownComplete, "SHUTDOWN-COMPLETE"),
        (ChunkType::Reconfig, "RECONFIG"),
        (ChunkType::ForwardTsn, "FORWARD-TSN"),
        (ChunkType::Unknown, "Unknown ChunkType"),
    ];

    for (ct, expected) in tests {
        assert_eq!(
            ct.to_string(),
            expected,
            "failed to stringify chunkType {}, expected {}",
            ct,
            expected
        );
    }

    Ok(())
}

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

///////////////////////////////////////////////////////////////////
//chunk_shutdown_test
///////////////////////////////////////////////////////////////////
use super::chunk_shutdown::*;

#[test]
fn test_chunk_shutdown_success() -> Result<(), Error> {
    let tests = vec![Bytes::from_static(&[
        0x07, 0x00, 0x00, 0x08, 0x12, 0x34, 0x56, 0x78,
    ])];

    for binary in tests {
        let actual = ChunkShutdown::unmarshal(&binary)?;
        let b = actual.marshal()?;
        assert_eq!(binary, b, "test not equal");
    }

    Ok(())
}

#[test]
fn test_chunk_shutdown_failure() -> Result<(), Error> {
    let tests = vec![
        (
            "length too short",
            Bytes::from_static(&[0x07, 0x00, 0x00, 0x07, 0x12, 0x34, 0x56, 0x78]),
        ),
        (
            "length too long",
            Bytes::from_static(&[0x07, 0x00, 0x00, 0x09, 0x12, 0x34, 0x56, 0x78]),
        ),
        (
            "payload too short",
            Bytes::from_static(&[0x07, 0x00, 0x00, 0x08, 0x12, 0x34, 0x56]),
        ),
        (
            "payload too long",
            Bytes::from_static(&[0x07, 0x00, 0x00, 0x08, 0x12, 0x34, 0x56, 0x78, 0x9f]),
        ),
        (
            "invalid type",
            Bytes::from_static(&[0x08, 0x00, 0x00, 0x08, 0x12, 0x34, 0x56, 0x78]),
        ),
    ];

    for (name, binary) in tests {
        let result = ChunkShutdown::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {} to fail.", name);
    }

    Ok(())
}

///////////////////////////////////////////////////////////////////
//chunk_shutdown_ack_test
///////////////////////////////////////////////////////////////////
use super::chunk_shutdown_ack::*;

#[test]
fn test_chunk_shutdown_ack_success() -> Result<(), Error> {
    let tests = vec![Bytes::from_static(&[0x08, 0x00, 0x00, 0x04])];

    for binary in tests {
        let actual = ChunkShutdownAck::unmarshal(&binary)?;
        let b = actual.marshal()?;
        assert_eq!(binary, b, "test not equal");
    }

    Ok(())
}

#[test]
fn test_chunk_shutdown_ack_failure() -> Result<(), Error> {
    let tests = vec![
        ("length too short", Bytes::from_static(&[0x08, 0x00, 0x00])),
        (
            "length too long",
            Bytes::from_static(&[0x08, 0x00, 0x00, 0x04, 0x12]),
        ),
        (
            "invalid type",
            Bytes::from_static(&[0x0f, 0x00, 0x00, 0x04]),
        ),
    ];

    for (name, binary) in tests {
        let result = ChunkShutdownAck::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {} to fail.", name);
    }

    Ok(())
}

///////////////////////////////////////////////////////////////////
//chunk_shutdown_complete_test
///////////////////////////////////////////////////////////////////
use super::chunk_shutdown_complete::*;

#[test]
fn test_chunk_shutdown_complete_success() -> Result<(), Error> {
    let tests = vec![Bytes::from_static(&[0x0e, 0x00, 0x00, 0x04])];

    for binary in tests {
        let actual = ChunkShutdownComplete::unmarshal(&binary)?;
        let b = actual.marshal()?;
        assert_eq!(binary, b, "test not equal");
    }

    Ok(())
}

#[test]
fn test_chunk_shutdown_complete_failure() -> Result<(), Error> {
    let tests = vec![
        ("length too short", Bytes::from_static(&[0x0e, 0x00, 0x00])),
        (
            "length too long",
            Bytes::from_static(&[0x0e, 0x00, 0x00, 0x04, 0x12]),
        ),
        (
            "invalid type",
            Bytes::from_static(&[0x0f, 0x00, 0x00, 0x04]),
        ),
    ];

    for (name, binary) in tests {
        let result = ChunkShutdownComplete::unmarshal(&binary);
        assert!(result.is_err(), "expected unmarshal: {} to fail.", name);
    }

    Ok(())
}

///////////////////////////////////////////////////////////////////
//chunk_test
///////////////////////////////////////////////////////////////////
/*
#[test]
fn TestInitChunk() -> Result<(), Error> {

    let rawPkt = Bytes::from_static(&[
        0x13, 0x88, 0x13, 0x88, 0x00, 0x00, 0x00, 0x00, 0x81, 0x46, 0x9d, 0xfc, 0x01, 0x00, 0x00, 0x56, 0x55,
        0xb9, 0x64, 0xa5, 0x00, 0x02, 0x00, 0x00, 0x04, 0x00, 0x08, 0x00, 0xe8, 0x6d, 0x10, 0x30, 0xc0, 0x00, 0x00, 0x04, 0x80,
        0x08, 0x00, 0x09, 0xc0, 0x0f, 0xc1, 0x80, 0x82, 0x00, 0x00, 0x00, 0x80, 0x02, 0x00, 0x24, 0x9f, 0xeb, 0xbb, 0x5c, 0x50,
        0xc9, 0xbf, 0x75, 0x9c, 0xb1, 0x2c, 0x57, 0x4f, 0xa4, 0x5a, 0x51, 0xba, 0x60, 0x17, 0x78, 0x27, 0x94, 0x5c, 0x31, 0xe6,
        0x5d, 0x5b, 0x09, 0x47, 0xe2, 0x22, 0x06, 0x80, 0x04, 0x00, 0x06, 0x00, 0x01, 0x00, 0x00, 0x80, 0x03, 0x00, 0x06, 0x80, 0xc1, 0x00, 0x00,
    ]);
    let pkt = Packet::unmarshal(&rawPkt)?;

    i, ok := pkt.chunks[0].(*chunkInit)
    if !ok {
        t.Errorf("Failed to cast Chunk -> Init")
    }

    switch {
    case err != nil:
        t.Errorf("Unmarshal init Chunk failed: %v", err)
    case i.initiateTag != 1438213285:
        t.Errorf("Unmarshal passed for SCTP packet, but got incorrect initiate tag exp: %d act: %d", 1438213285, i.initiateTag)
    case i.advertisedReceiverWindowCredit != 131072:
        t.Errorf("Unmarshal passed for SCTP packet, but got incorrect advertisedReceiverWindowCredit exp: %d act: %d", 131072, i.advertisedReceiverWindowCredit)
    case i.numOutboundStreams != 1024:
        t.Errorf("Unmarshal passed for SCTP packet, but got incorrect numOutboundStreams tag exp: %d act: %d", 1024, i.numOutboundStreams)
    case i.numInboundStreams != 2048:
        t.Errorf("Unmarshal passed for SCTP packet, but got incorrect numInboundStreams exp: %d act: %d", 2048, i.numInboundStreams)
    case i.initialTSN != uint32(3899461680):
        t.Errorf("Unmarshal passed for SCTP packet, but got incorrect initialTSN exp: %d act: %d", uint32(3899461680), i.initialTSN)
    }

    Ok(())
}

func TestInitAck(t *testing.T) {
    pkt := &packet{}
    rawPkt := []byte{0x13, 0x88, 0x13, 0x88, 0xce, 0x15, 0x79, 0xa2, 0x96, 0x19, 0xe8, 0xb2, 0x02, 0x00, 0x00, 0x1c, 0xeb, 0x81, 0x4e, 0x01, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x08, 0x00, 0x50, 0xdf, 0x90, 0xd9, 0x00, 0x07, 0x00, 0x08, 0x94, 0x06, 0x2f, 0x93}
    err := pkt.unmarshal(rawPkt)
    if err != nil {
        t.Errorf("Unmarshal failed, has chunk: %v", err)
    }

    _, ok := pkt.chunks[0].(*chunkInitAck)
    if !ok {
        t.Error("Failed to cast Chunk -> Init")
    } else if err != nil {
        t.Errorf("Unmarshal init Chunk failed: %v", err)
    }
}

func TestChromeChunk1Init(t *testing.T) {
    pkt := &packet{}
    rawPkt := []byte{0x13, 0x88, 0x13, 0x88, 0x00, 0x00, 0x00, 0x00, 0xbc, 0xb3, 0x45, 0xa2, 0x01, 0x00, 0x00, 0x56, 0xce, 0x15, 0x79, 0xa2, 0x00, 0x02, 0x00, 0x00, 0x04, 0x00, 0x08, 0x00, 0x94, 0x57, 0x95, 0xc0, 0xc0, 0x00, 0x00, 0x04, 0x80, 0x08, 0x00, 0x09, 0xc0, 0x0f, 0xc1, 0x80, 0x82, 0x00, 0x00, 0x00, 0x80, 0x02, 0x00, 0x24, 0xff, 0x5c, 0x49, 0x19, 0x4a, 0x94, 0xe8, 0x2a, 0xec, 0x58, 0x55, 0x62, 0x29, 0x1f, 0x8e, 0x23, 0xcd, 0x7c, 0xe8, 0x46, 0xba, 0x58, 0x1b, 0x3d, 0xab, 0xd7, 0x7e, 0x50, 0xf2, 0x41, 0xb1, 0x2e, 0x80, 0x04, 0x00, 0x06, 0x00, 0x01, 0x00, 0x00, 0x80, 0x03, 0x00, 0x06, 0x80, 0xc1, 0x00, 0x00}
    err := pkt.unmarshal(rawPkt)
    if err != nil {
        t.Errorf("Unmarshal failed, has chunk: %v", err)
    }

    rawPkt2, err := pkt.marshal()
    if err != nil {
        t.Errorf("Remarshal failed: %v", err)
    }

    assert.Equal(t, rawPkt, rawPkt2)
}

func TestChromeChunk2InitAck(t *testing.T) {
    pkt := &packet{}
    rawPkt := []byte{0x13, 0x88, 0x13, 0x88, 0xce, 0x15, 0x79, 0xa2, 0xb5, 0xdb, 0x2d, 0x93, 0x02, 0x00, 0x01, 0x90, 0x9b, 0xd5, 0xb3, 0x6f, 0x00, 0x02, 0x00, 0x00, 0x04, 0x00, 0x08, 0x00, 0xef, 0xb4, 0x72, 0x87, 0xc0, 0x00, 0x00, 0x04, 0x80, 0x08, 0x00, 0x09, 0xc0, 0x0f, 0xc1, 0x80, 0x82, 0x00, 0x00, 0x00, 0x80, 0x02, 0x00, 0x24, 0x2e, 0xf9, 0x9c, 0x10, 0x63, 0x72, 0xed, 0x0d, 0x33, 0xc2, 0xdc, 0x7f, 0x9f, 0xd7, 0xef, 0x1b, 0xc9, 0xc4, 0xa7, 0x41, 0x9a, 0x07, 0x68, 0x6b, 0x66, 0xfb, 0x6a, 0x4e, 0x32, 0x5d, 0xe4, 0x25, 0x80, 0x04, 0x00, 0x06, 0x00, 0x01, 0x00, 0x00, 0x80, 0x03, 0x00, 0x06, 0x80, 0xc1, 0x00, 0x00, 0x00, 0x07, 0x01, 0x38, 0x4b, 0x41, 0x4d, 0x45, 0x2d, 0x42, 0x53, 0x44, 0x20, 0x31, 0x2e, 0x31, 0x00, 0x00, 0x00, 0x00, 0x9c, 0x1e, 0x49, 0x5b, 0x00, 0x00, 0x00, 0x00, 0xd2, 0x42, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x60, 0xea, 0x00, 0x00, 0xc4, 0x13, 0x3d, 0xe9, 0x86, 0xb1, 0x85, 0x75, 0xa2, 0x79, 0x15, 0xce, 0x9b, 0xd5, 0xb3, 0x6f, 0x20, 0xe0, 0x9f, 0x89, 0xe0, 0x27, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x20, 0xe0, 0x9f, 0x89, 0xe0, 0x27, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x13, 0x88, 0x13, 0x88, 0x00, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x56, 0xce, 0x15, 0x79, 0xa2, 0x00, 0x02, 0x00, 0x00, 0x04, 0x00, 0x08, 0x00, 0x94, 0x57, 0x95, 0xc0, 0xc0, 0x00, 0x00, 0x04, 0x80, 0x08, 0x00, 0x09, 0xc0, 0x0f, 0xc1, 0x80, 0x82, 0x00, 0x00, 0x00, 0x80, 0x02, 0x00, 0x24, 0xff, 0x5c, 0x49, 0x19, 0x4a, 0x94, 0xe8, 0x2a, 0xec, 0x58, 0x55, 0x62, 0x29, 0x1f, 0x8e, 0x23, 0xcd, 0x7c, 0xe8, 0x46, 0xba, 0x58, 0x1b, 0x3d, 0xab, 0xd7, 0x7e, 0x50, 0xf2, 0x41, 0xb1, 0x2e, 0x80, 0x04, 0x00, 0x06, 0x00, 0x01, 0x00, 0x00, 0x80, 0x03, 0x00, 0x06, 0x80, 0xc1, 0x00, 0x00, 0x02, 0x00, 0x01, 0x90, 0x9b, 0xd5, 0xb3, 0x6f, 0x00, 0x02, 0x00, 0x00, 0x04, 0x00, 0x08, 0x00, 0xef, 0xb4, 0x72, 0x87, 0xc0, 0x00, 0x00, 0x04, 0x80, 0x08, 0x00, 0x09, 0xc0, 0x0f, 0xc1, 0x80, 0x82, 0x00, 0x00, 0x00, 0x80, 0x02, 0x00, 0x24, 0x2e, 0xf9, 0x9c, 0x10, 0x63, 0x72, 0xed, 0x0d, 0x33, 0xc2, 0xdc, 0x7f, 0x9f, 0xd7, 0xef, 0x1b, 0xc9, 0xc4, 0xa7, 0x41, 0x9a, 0x07, 0x68, 0x6b, 0x66, 0xfb, 0x6a, 0x4e, 0x32, 0x5d, 0xe4, 0x25, 0x80, 0x04, 0x00, 0x06, 0x00, 0x01, 0x00, 0x00, 0x80, 0x03, 0x00, 0x06, 0x80, 0xc1, 0x00, 0x00, 0xca, 0x0c, 0x21, 0x11, 0xce, 0xf4, 0xfc, 0xb3, 0x66, 0x99, 0x4f, 0xdb, 0x4f, 0x95, 0x6b, 0x6f, 0x3b, 0xb1, 0xdb, 0x5a}
    err := pkt.unmarshal(rawPkt)
    if err != nil {
        t.Errorf("Unmarshal failed, has chunk: %v", err)
    }

    rawPkt2, err := pkt.marshal()
    if err != nil {
        t.Errorf("Remarshal failed: %v", err)
    }

    assert.Equal(t, rawPkt, rawPkt2)
}

func TestInitMarshalUnmarshal(t *testing.T) {
    p := &packet{}
    p.destinationPort = 1
    p.sourcePort = 1
    p.verificationTag = 123

    initAck := &chunkInitAck{}

    initAck.initialTSN = 123
    initAck.numOutboundStreams = 1
    initAck.numInboundStreams = 1
    initAck.initiateTag = 123
    initAck.advertisedReceiverWindowCredit = 1024
    cookie, errRand := newRandomStateCookie()
    if errRand != nil {
        t.Fatalf("Failed to generate random state cookie: %v", errRand)
    }
    initAck.params = []param{cookie}

    p.chunks = []chunk{initAck}
    rawPkt, err := p.marshal()
    if err != nil {
        t.Errorf("Failed to marshal packet: %v", err)
    }

    pkt := &packet{}
    err = pkt.unmarshal(rawPkt)
    if err != nil {
        t.Errorf("Unmarshal failed, has chunk: %v", err)
    }

    i, ok := pkt.chunks[0].(*chunkInitAck)
    if !ok {
        t.Error("Failed to cast Chunk -> InitAck")
    }

    switch {
    case err != nil:
        t.Errorf("Unmarshal init ack Chunk failed: %v", err)
    case i.initiateTag != 123:
        t.Errorf("Unmarshal passed for SCTP packet, but got incorrect initiate tag exp: %d act: %d", 123, i.initiateTag)
    case i.advertisedReceiverWindowCredit != 1024:
        t.Errorf("Unmarshal passed for SCTP packet, but got incorrect advertisedReceiverWindowCredit exp: %d act: %d", 1024, i.advertisedReceiverWindowCredit)
    case i.numOutboundStreams != 1:
        t.Errorf("Unmarshal passed for SCTP packet, but got incorrect numOutboundStreams tag exp: %d act: %d", 1, i.numOutboundStreams)
    case i.numInboundStreams != 1:
        t.Errorf("Unmarshal passed for SCTP packet, but got incorrect numInboundStreams exp: %d act: %d", 1, i.numInboundStreams)
    case i.initialTSN != 123:
        t.Errorf("Unmarshal passed for SCTP packet, but got incorrect initialTSN exp: %d act: %d", 123, i.initialTSN)
    }
}

func TestPayloadDataMarshalUnmarshal(t *testing.T) {
    pkt := &packet{}
    rawPkt := []byte{0x13, 0x88, 0x13, 0x88, 0xfc, 0xd6, 0x3f, 0xc6, 0xbe, 0xfa, 0xdc, 0x52, 0x0a, 0x00, 0x00, 0x24, 0x9b, 0x28, 0x7e, 0x48, 0xa3, 0x7b, 0xc1, 0x83, 0xc4, 0x4b, 0x41, 0x04, 0xa4, 0xf7, 0xed, 0x4c, 0x93, 0x62, 0xc3, 0x49, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x1f, 0xa8, 0x79, 0xa1, 0xc7, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x32, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x66, 0x6f, 0x6f, 0x00}
    err := pkt.unmarshal(rawPkt)
    if err != nil {
        t.Errorf("Unmarshal failed, has chunk: %v", err)
    }

    _, ok := pkt.chunks[1].(*chunkPayloadData)
    if !ok {
        t.Error("Failed to cast Chunk -> PayloadData")
    }
}

func TestSelectAckChunk(t *testing.T) {
    pkt := &packet{}
    rawPkt := []byte{0x13, 0x88, 0x13, 0x88, 0xc2, 0x98, 0x98, 0x0f, 0x42, 0x31, 0xea, 0x78, 0x03, 0x00, 0x00, 0x14, 0x87, 0x73, 0xbd, 0xa4, 0x00, 0x01, 0xfe, 0x74, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02}
    err := pkt.unmarshal(rawPkt)
    if err != nil {
        t.Errorf("Unmarshal failed, has chunk: %v", err)
    }

    _, ok := pkt.chunks[0].(*chunkSelectiveAck)
    if !ok {
        t.Error("Failed to cast Chunk -> SelectiveAck")
    }
}

func TestReconfigChunk(t *testing.T) {
    pkt := &packet{}
    rawPkt := []byte{0x13, 0x88, 0x13, 0x88, 0xb6, 0xa5, 0x12, 0xe5, 0x75, 0x3b, 0x12, 0xd3, 0x82, 0x0, 0x0, 0x16, 0x0, 0xd, 0x0, 0x12, 0x4e, 0x1c, 0xb9, 0xe6, 0x3a, 0x74, 0x8d, 0xff, 0x4e, 0x1c, 0xb9, 0xe6, 0x0, 0x1, 0x0, 0x0}
    err := pkt.unmarshal(rawPkt)
    if err != nil {
        t.Errorf("Unmarshal failed, has chunk: %v", err)
    }

    c, ok := pkt.chunks[0].(*chunkReconfig)
    if !ok {
        t.Error("Failed to cast Chunk -> Reconfig")
    }

    if c.paramA.(*paramOutgoingResetRequest).streamIdentifiers[0] != uint16(1) {
        t.Errorf("unexpected stream identifier: %d", c.paramA.(*paramOutgoingResetRequest).streamIdentifiers[0])
    }
}

func TestForwardTSNChunk(t *testing.T) {
    pkt := &packet{}
    rawPkt := append([]byte{0x13, 0x88, 0x13, 0x88, 0xb6, 0xa5, 0x12, 0xe5, 0x1f, 0x9d, 0xa0, 0xfb}, testChunkForwardTSN()...)
    err := pkt.unmarshal(rawPkt)
    if err != nil {
        t.Errorf("Unmarshal failed, has chunk: %v", err)
    }

    c, ok := pkt.chunks[0].(*chunkForwardTSN)
    if !ok {
        t.Error("Failed to cast Chunk -> Forward TSN")
    }

    if c.newCumulativeTSN != uint32(3) {
        t.Errorf("unexpected New Cumulative TSN: %d", c.newCumulativeTSN)
    }
}
*/
