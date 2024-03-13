use bytes::{Bytes, BytesMut};

///////////////////////////////////////////////////////////////////
//payload_queue_test
///////////////////////////////////////////////////////////////////
use super::payload_queue::*;
use crate::chunk::chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier};
use crate::chunk::chunk_selective_ack::GapAckBlock;
use crate::error::{Error, Result};

fn make_payload(tsn: u32, n_bytes: usize) -> ChunkPayloadData {
    ChunkPayloadData {
        tsn,
        user_data: {
            let mut b = BytesMut::new();
            b.resize(n_bytes, 0);
            b.freeze()
        },
        ..Default::default()
    }
}

#[test]
fn test_payload_queue_push_no_check() -> Result<()> {
    let mut pq = PayloadQueue::new(Arc::new(AtomicUsize::new(0)));

    pq.push_no_check(make_payload(0, 10));
    assert_eq!(pq.get_num_bytes(), 10, "total bytes mismatch");
    assert_eq!(pq.len(), 1, "item count mismatch");
    pq.push_no_check(make_payload(1, 11));
    assert_eq!(pq.get_num_bytes(), 21, "total bytes mismatch");
    assert_eq!(pq.len(), 2, "item count mismatch");
    pq.push_no_check(make_payload(2, 12));
    assert_eq!(pq.get_num_bytes(), 33, "total bytes mismatch");
    assert_eq!(pq.len(), 3, "item count mismatch");

    for i in 0..3 {
        assert!(!pq.sorted.is_empty(), "should not be empty");
        let c = pq.pop(i);
        assert!(c.is_some(), "pop should succeed");
        if let Some(c) = c {
            assert_eq!(c.tsn, i, "TSN should match");
        }
    }

    assert_eq!(pq.get_num_bytes(), 0, "total bytes mismatch");
    assert_eq!(pq.len(), 0, "item count mismatch");

    assert!(pq.sorted.is_empty(), "should be empty");
    pq.push_no_check(make_payload(3, 13));
    assert_eq!(pq.get_num_bytes(), 13, "total bytes mismatch");
    pq.push_no_check(make_payload(4, 14));
    assert_eq!(pq.get_num_bytes(), 27, "total bytes mismatch");

    for i in 3..5 {
        assert!(!pq.sorted.is_empty(), "should not be empty");
        let c = pq.pop(i);
        assert!(c.is_some(), "pop should succeed");
        if let Some(c) = c {
            assert_eq!(c.tsn, i, "TSN should match");
        }
    }

    assert_eq!(pq.get_num_bytes(), 0, "total bytes mismatch");
    assert_eq!(pq.len(), 0, "item count mismatch");

    Ok(())
}

#[test]
fn test_payload_queue_get_gap_ack_block() -> Result<()> {
    let mut pq = PayloadQueue::new(Arc::new(AtomicUsize::new(0)));

    pq.push(make_payload(1, 0), 0);
    pq.push(make_payload(2, 0), 0);
    pq.push(make_payload(3, 0), 0);
    pq.push(make_payload(4, 0), 0);
    pq.push(make_payload(5, 0), 0);
    pq.push(make_payload(6, 0), 0);

    let gab1 = [GapAckBlock { start: 1, end: 6 }];
    let gab2 = pq.get_gap_ack_blocks(0);
    assert!(!gab2.is_empty());
    assert_eq!(gab2.len(), 1);

    assert_eq!(gab2[0].start, gab1[0].start);
    assert_eq!(gab2[0].end, gab1[0].end);

    pq.push(make_payload(8, 0), 0);
    pq.push(make_payload(9, 0), 0);

    let gab1 = [
        GapAckBlock { start: 1, end: 6 },
        GapAckBlock { start: 8, end: 9 },
    ];
    let gab2 = pq.get_gap_ack_blocks(0);
    assert!(!gab2.is_empty());
    assert_eq!(gab2.len(), 2);

    assert_eq!(gab2[0].start, gab1[0].start);
    assert_eq!(gab2[0].end, gab1[0].end);
    assert_eq!(gab2[1].start, gab1[1].start);
    assert_eq!(gab2[1].end, gab1[1].end);

    Ok(())
}

#[test]
fn test_payload_queue_get_last_tsn_received() -> Result<()> {
    let mut pq = PayloadQueue::new(Arc::new(AtomicUsize::new(0)));

    // empty queie should return false
    let ok = pq.get_last_tsn_received();
    assert!(ok.is_none(), "should be none");

    let ok = pq.push(make_payload(20, 0), 0);
    assert!(ok, "should be true");
    let tsn = pq.get_last_tsn_received();
    assert!(tsn.is_some(), "should be false");
    assert_eq!(tsn, Some(&20), "should match");

    // append should work
    let ok = pq.push(make_payload(21, 0), 0);
    assert!(ok, "should be true");
    let tsn = pq.get_last_tsn_received();
    assert!(tsn.is_some(), "should be false");
    assert_eq!(tsn, Some(&21), "should match");

    // check if sorting applied
    let ok = pq.push(make_payload(19, 0), 0);
    assert!(ok, "should be true");
    let tsn = pq.get_last_tsn_received();
    assert!(tsn.is_some(), "should be false");
    assert_eq!(tsn, Some(&21), "should match");

    Ok(())
}

#[test]
fn test_payload_queue_mark_all_to_retrasmit() -> Result<()> {
    let mut pq = PayloadQueue::new(Arc::new(AtomicUsize::new(0)));

    for i in 0..3 {
        pq.push(make_payload(i + 1, 10), 0);
    }
    pq.mark_as_acked(2);
    pq.mark_all_to_retrasmit();

    let c = pq.get(1);
    assert!(c.is_some(), "should be true");
    assert!(c.unwrap().retransmit, "should be marked as retransmit");
    let c = pq.get(2);
    assert!(c.is_some(), "should be true");
    assert!(!c.unwrap().retransmit, "should NOT be marked as retransmit");
    let c = pq.get(3);
    assert!(c.is_some(), "should be true");
    assert!(c.unwrap().retransmit, "should be marked as retransmit");

    Ok(())
}

#[test]
fn test_payload_queue_reset_retransmit_flag_on_ack() -> Result<()> {
    let mut pq = PayloadQueue::new(Arc::new(AtomicUsize::new(0)));

    for i in 0..4 {
        pq.push(make_payload(i + 1, 10), 0);
    }

    pq.mark_all_to_retrasmit();
    pq.mark_as_acked(2); // should cancel retransmission for TSN 2
    pq.mark_as_acked(4); // should cancel retransmission for TSN 4

    let c = pq.get(1);
    assert!(c.is_some(), "should be true");
    assert!(c.unwrap().retransmit, "should be marked as retransmit");
    let c = pq.get(2);
    assert!(c.is_some(), "should be true");
    assert!(!c.unwrap().retransmit, "should NOT be marked as retransmit");
    let c = pq.get(3);
    assert!(c.is_some(), "should be true");
    assert!(c.unwrap().retransmit, "should be marked as retransmit");
    let c = pq.get(4);
    assert!(c.is_some(), "should be true");
    assert!(!c.unwrap().retransmit, "should NOT be marked as retransmit");

    Ok(())
}

///////////////////////////////////////////////////////////////////
//pending_queue_test
///////////////////////////////////////////////////////////////////
use super::pending_queue::*;

const NO_FRAGMENT: usize = 0;
const FRAG_BEGIN: usize = 1;
const FRAG_MIDDLE: usize = 2;
const FRAG_END: usize = 3;

fn make_data_chunk(tsn: u32, unordered: bool, frag: usize) -> ChunkPayloadData {
    let mut b = false;
    let mut e = false;

    match frag {
        NO_FRAGMENT => {
            b = true;
            e = true;
        }
        FRAG_BEGIN => {
            b = true;
        }
        FRAG_END => e = true,
        _ => {}
    };

    ChunkPayloadData {
        tsn,
        unordered,
        beginning_fragment: b,
        ending_fragment: e,
        user_data: {
            let mut b = BytesMut::new();
            b.resize(10, 0); // always 10 bytes
            b.freeze()
        },
        ..Default::default()
    }
}

#[test]
fn test_pending_base_queue_push_and_pop() -> Result<()> {
    let mut pq = PendingBaseQueue::new();
    pq.push_back(make_data_chunk(0, false, NO_FRAGMENT));
    pq.push_back(make_data_chunk(1, false, NO_FRAGMENT));
    pq.push_back(make_data_chunk(2, false, NO_FRAGMENT));

    for i in 0..3 {
        let c = pq.get(i);
        assert!(c.is_some(), "should not be none");
        assert_eq!(c.unwrap().tsn, i as u32, "TSN should match");
    }

    for i in 0..3 {
        let c = pq.pop_front();
        assert!(c.is_some(), "should not be none");
        assert_eq!(c.unwrap().tsn, i, "TSN should match");
    }

    pq.push_back(make_data_chunk(3, false, NO_FRAGMENT));
    pq.push_back(make_data_chunk(4, false, NO_FRAGMENT));

    for i in 3..5 {
        let c = pq.pop_front();
        assert!(c.is_some(), "should not be none");
        assert_eq!(c.unwrap().tsn, i, "TSN should match");
    }
    Ok(())
}

#[test]
fn test_pending_base_queue_out_of_bounce() -> Result<()> {
    let mut pq = PendingBaseQueue::new();
    assert!(pq.pop_front().is_none(), "should be none");
    assert!(pq.front().is_none(), "should be none");

    pq.push_back(make_data_chunk(0, false, NO_FRAGMENT));
    assert!(pq.get(1).is_none(), "should be none");

    Ok(())
}

// NOTE: TSN is not used in pendingQueue in the actual usage.
//       Following tests use TSN field as a chunk ID.
#[tokio::test]
async fn test_pending_queue_push_and_pop() -> Result<()> {
    let pq = PendingQueue::new();
    pq.push(make_data_chunk(0, false, NO_FRAGMENT)).await;
    assert_eq!(pq.get_num_bytes(), 10, "total bytes mismatch");
    pq.push(make_data_chunk(1, false, NO_FRAGMENT)).await;
    assert_eq!(pq.get_num_bytes(), 20, "total bytes mismatch");
    pq.push(make_data_chunk(2, false, NO_FRAGMENT)).await;
    assert_eq!(pq.get_num_bytes(), 30, "total bytes mismatch");

    for i in 0..3 {
        let c = pq.peek();
        assert!(c.is_some(), "peek error");
        let c = c.unwrap();
        assert_eq!(c.tsn, i, "TSN should match");
        let (beginning_fragment, unordered) = (c.beginning_fragment, c.unordered);

        let result = pq.pop(beginning_fragment, unordered);
        assert!(result.is_some(), "should not error: {i}");
    }

    assert_eq!(pq.get_num_bytes(), 0, "total bytes mismatch");

    pq.push(make_data_chunk(3, false, NO_FRAGMENT)).await;
    assert_eq!(pq.get_num_bytes(), 10, "total bytes mismatch");
    pq.push(make_data_chunk(4, false, NO_FRAGMENT)).await;
    assert_eq!(pq.get_num_bytes(), 20, "total bytes mismatch");

    for i in 3..5 {
        let c = pq.peek();
        assert!(c.is_some(), "peek error");
        let c = c.unwrap();
        assert_eq!(c.tsn, i, "TSN should match");
        let (beginning_fragment, unordered) = (c.beginning_fragment, c.unordered);

        let result = pq.pop(beginning_fragment, unordered);
        assert!(result.is_some(), "should not error: {i}");
    }

    assert_eq!(pq.get_num_bytes(), 0, "total bytes mismatch");

    Ok(())
}

#[tokio::test]
async fn test_pending_queue_unordered_wins() -> Result<()> {
    let pq = PendingQueue::new();

    pq.push(make_data_chunk(0, false, NO_FRAGMENT)).await;
    assert_eq!(10, pq.get_num_bytes(), "total bytes mismatch");
    pq.push(make_data_chunk(1, true, NO_FRAGMENT)).await;
    assert_eq!(20, pq.get_num_bytes(), "total bytes mismatch");
    pq.push(make_data_chunk(2, false, NO_FRAGMENT)).await;
    assert_eq!(30, pq.get_num_bytes(), "total bytes mismatch");
    pq.push(make_data_chunk(3, true, NO_FRAGMENT)).await;
    assert_eq!(40, pq.get_num_bytes(), "total bytes mismatch");

    let c = pq.peek();
    assert!(c.is_some(), "peek error");
    let c = c.unwrap();
    assert_eq!(c.tsn, 1, "TSN should match");
    let (beginning_fragment, unordered) = (c.beginning_fragment, c.unordered);
    let result = pq.pop(beginning_fragment, unordered);
    assert!(result.is_some(), "should not error");

    let c = pq.peek();
    assert!(c.is_some(), "peek error");
    let c = c.unwrap();
    assert_eq!(c.tsn, 3, "TSN should match");
    let (beginning_fragment, unordered) = (c.beginning_fragment, c.unordered);
    let result = pq.pop(beginning_fragment, unordered);
    assert!(result.is_some(), "should not error");

    let c = pq.peek();
    assert!(c.is_some(), "peek error");
    let c = c.unwrap();
    assert_eq!(c.tsn, 0, "TSN should match");
    let (beginning_fragment, unordered) = (c.beginning_fragment, c.unordered);
    let result = pq.pop(beginning_fragment, unordered);
    assert!(result.is_some(), "should not error");

    let c = pq.peek();
    assert!(c.is_some(), "peek error");
    let c = c.unwrap();
    assert_eq!(c.tsn, 2, "TSN should match");
    let (beginning_fragment, unordered) = (c.beginning_fragment, c.unordered);
    let result = pq.pop(beginning_fragment, unordered);
    assert!(result.is_some(), "should not error");

    assert_eq!(pq.get_num_bytes(), 0, "total bytes mismatch");

    Ok(())
}

#[tokio::test]
async fn test_pending_queue_fragments() -> Result<()> {
    let pq = PendingQueue::new();
    pq.push(make_data_chunk(0, false, FRAG_BEGIN)).await;
    pq.push(make_data_chunk(1, false, FRAG_MIDDLE)).await;
    pq.push(make_data_chunk(2, false, FRAG_END)).await;
    pq.push(make_data_chunk(3, true, FRAG_BEGIN)).await;
    pq.push(make_data_chunk(4, true, FRAG_MIDDLE)).await;
    pq.push(make_data_chunk(5, true, FRAG_END)).await;

    let expects = vec![3, 4, 5, 0, 1, 2];

    for exp in expects {
        let c = pq.peek();
        assert!(c.is_some(), "peek error");
        let c = c.unwrap();
        assert_eq!(c.tsn, exp, "TSN should match");
        let (beginning_fragment, unordered) = (c.beginning_fragment, c.unordered);
        let result = pq.pop(beginning_fragment, unordered);
        assert!(result.is_some(), "should not error: {exp}");
    }

    Ok(())
}

// Once decided ordered or unordered, the decision should persist until
// it pops a chunk with ending_fragment flags set to true.
#[tokio::test]
async fn test_pending_queue_selection_persistence() -> Result<()> {
    let pq = PendingQueue::new();
    pq.push(make_data_chunk(0, false, FRAG_BEGIN)).await;

    let c = pq.peek();
    assert!(c.is_some(), "peek error");
    let c = c.unwrap();
    assert_eq!(c.tsn, 0, "TSN should match");
    let (beginning_fragment, unordered) = (c.beginning_fragment, c.unordered);
    let result = pq.pop(beginning_fragment, unordered);
    assert!(result.is_some(), "should not error: {}", 0);

    pq.push(make_data_chunk(1, true, NO_FRAGMENT)).await;
    pq.push(make_data_chunk(2, false, FRAG_MIDDLE)).await;
    pq.push(make_data_chunk(3, false, FRAG_END)).await;

    let expects = vec![2, 3, 1];

    for exp in expects {
        let c = pq.peek();
        assert!(c.is_some(), "peek error");
        let c = c.unwrap();
        assert_eq!(c.tsn, exp, "TSN should match");
        let (beginning_fragment, unordered) = (c.beginning_fragment, c.unordered);
        let result = pq.pop(beginning_fragment, unordered);
        assert!(result.is_some(), "should not error: {exp}");
    }

    Ok(())
}

#[tokio::test]
async fn test_pending_queue_append() -> Result<()> {
    let pq = PendingQueue::new();
    pq.append(vec![
        make_data_chunk(0, false, NO_FRAGMENT),
        make_data_chunk(1, false, NO_FRAGMENT),
        make_data_chunk(3, false, NO_FRAGMENT),
    ])
    .await;
    assert_eq!(pq.get_num_bytes(), 30, "total bytes mismatch");
    assert_eq!(pq.len(), 3, "len mismatch");

    Ok(())
}

///////////////////////////////////////////////////////////////////
//reassembly_queue_test
///////////////////////////////////////////////////////////////////
use std::sync::Arc;

use portable_atomic::AtomicUsize;

use super::reassembly_queue::*;

#[test]
fn test_reassembly_queue_ordered_fragments() -> Result<()> {
    let mut rq = ReassemblyQueue::new(0);

    let org_ppi = PayloadProtocolIdentifier::Binary;

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        beginning_fragment: true,
        tsn: 1,
        stream_sequence_number: 0,
        user_data: Bytes::from_static(b"ABC"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk set should not be complete yet");
    assert_eq!(rq.get_num_bytes(), 3, "num bytes mismatch");

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        ending_fragment: true,
        tsn: 2,
        stream_sequence_number: 0,
        user_data: Bytes::from_static(b"DEFG"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(complete, "chunk set should be complete");
    assert_eq!(7, rq.get_num_bytes(), "num bytes mismatch");

    let mut buf = vec![0u8; 16];

    let (n, ppi) = rq.read(&mut buf)?;
    assert_eq!(n, 7, "should received 7 bytes");
    assert_eq!(rq.get_num_bytes(), 0, "num bytes mismatch");
    assert_eq!(ppi, org_ppi, "should have valid ppi");
    assert_eq!(&buf[..n], b"ABCDEFG", "data should match");

    Ok(())
}

#[test]
fn test_reassembly_queue_unordered_fragments() -> Result<()> {
    let mut rq = ReassemblyQueue::new(0);

    let org_ppi = PayloadProtocolIdentifier::Binary;

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        unordered: true,
        beginning_fragment: true,
        tsn: 1,
        stream_sequence_number: 0,
        user_data: Bytes::from_static(b"ABC"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk set should not be complete yet");
    assert_eq!(rq.get_num_bytes(), 3, "num bytes mismatch");

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        unordered: true,
        tsn: 2,
        stream_sequence_number: 0,
        user_data: Bytes::from_static(b"DEFG"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk set should not be complete yet");
    assert_eq!(rq.get_num_bytes(), 7, "num bytes mismatch");

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        unordered: true,
        ending_fragment: true,
        tsn: 3,
        stream_sequence_number: 0,
        user_data: Bytes::from_static(b"H"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(complete, "chunk set should be complete");
    assert_eq!(rq.get_num_bytes(), 8, "num bytes mismatch");

    let mut buf = vec![0u8; 16];

    let (n, ppi) = rq.read(&mut buf)?;
    assert_eq!(n, 8, "should received 8 bytes");
    assert_eq!(rq.get_num_bytes(), 0, "num bytes mismatch");
    assert_eq!(ppi, org_ppi, "should have valid ppi");
    assert_eq!(&buf[..n], b"ABCDEFGH", "data should match");

    Ok(())
}

#[test]
fn test_reassembly_queue_ordered_and_unordered_fragments() -> Result<()> {
    let mut rq = ReassemblyQueue::new(0);
    let org_ppi = PayloadProtocolIdentifier::Binary;
    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 1,
        stream_sequence_number: 0,
        user_data: Bytes::from_static(b"ABC"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(complete, "chunk set should be complete");
    assert_eq!(rq.get_num_bytes(), 3, "num bytes mismatch");

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        unordered: true,
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 2,
        stream_sequence_number: 1,
        user_data: Bytes::from_static(b"DEF"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(complete, "chunk set should be complete");
    assert_eq!(rq.get_num_bytes(), 6, "num bytes mismatch");

    //
    // Now we have two complete chunks ready to read in the reassemblyQueue.
    //

    let mut buf = vec![0u8; 16];

    // Should read unordered chunks first
    let (n, ppi) = rq.read(&mut buf)?;
    assert_eq!(n, 3, "should received 3 bytes");
    assert_eq!(rq.get_num_bytes(), 3, "num bytes mismatch");
    assert_eq!(ppi, org_ppi, "should have valid ppi");
    assert_eq!(&buf[..n], b"DEF", "data should match");

    // Next should read ordered chunks
    let (n, ppi) = rq.read(&mut buf)?;
    assert_eq!(n, 3, "should received 3 bytes");
    assert_eq!(rq.get_num_bytes(), 0, "num bytes mismatch");
    assert_eq!(ppi, org_ppi, "should have valid ppi");
    assert_eq!(&buf[..n], b"ABC", "data should match");

    Ok(())
}

#[test]
fn test_reassembly_queue_unordered_complete_skips_incomplete() -> Result<()> {
    let mut rq = ReassemblyQueue::new(0);

    let org_ppi = PayloadProtocolIdentifier::Binary;

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        unordered: true,
        beginning_fragment: true,
        tsn: 10,
        stream_sequence_number: 0,
        user_data: Bytes::from_static(b"IN"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk set should not be complete yet");
    assert_eq!(2, rq.get_num_bytes(), "num bytes mismatch");

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        unordered: true,
        ending_fragment: true,
        tsn: 12, // <- incongiguous
        stream_sequence_number: 1,
        user_data: Bytes::from_static(b"COMPLETE"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk set should not be complete yet");
    assert_eq!(rq.get_num_bytes(), 10, "num bytes mismatch");

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        unordered: true,
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 13,
        stream_sequence_number: 1,
        user_data: Bytes::from_static(b"GOOD"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(complete, "chunk set should be complete");
    assert_eq!(rq.get_num_bytes(), 14, "num bytes mismatch");

    //
    // Now we have two complete chunks ready to read in the reassemblyQueue.
    //

    let mut buf = vec![0u8; 16];

    // Should pick the one that has "GOOD"
    let (n, ppi) = rq.read(&mut buf)?;
    assert_eq!(n, 4, "should receive 4 bytes");
    assert_eq!(rq.get_num_bytes(), 10, "num bytes mismatch");
    assert_eq!(ppi, org_ppi, "should have valid ppi");
    assert_eq!(&buf[..n], b"GOOD", "data should match");

    Ok(())
}

#[test]
fn test_reassembly_queue_ignores_chunk_with_wrong_si() -> Result<()> {
    let mut rq = ReassemblyQueue::new(123);

    let org_ppi = PayloadProtocolIdentifier::Binary;

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        stream_identifier: 124,
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 10,
        stream_sequence_number: 0,
        user_data: Bytes::from_static(b"IN"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk should be ignored");
    assert_eq!(rq.get_num_bytes(), 0, "num bytes mismatch");
    Ok(())
}

#[test]
fn test_reassembly_queue_ignores_chunk_with_stale_ssn() -> Result<()> {
    let mut rq = ReassemblyQueue::new(0);
    rq.next_ssn = 7; // forcibly set expected SSN to 7

    let org_ppi = PayloadProtocolIdentifier::Binary;

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 10,
        stream_sequence_number: 6, // <-- stale
        user_data: Bytes::from_static(b"IN"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk should not be ignored");
    assert_eq!(rq.get_num_bytes(), 0, "num bytes mismatch");

    Ok(())
}

#[test]
fn test_reassembly_queue_should_fail_to_read_incomplete_chunk() -> Result<()> {
    let mut rq = ReassemblyQueue::new(0);

    let org_ppi = PayloadProtocolIdentifier::Binary;

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        beginning_fragment: true,
        tsn: 123,
        stream_sequence_number: 0,
        user_data: Bytes::from_static(b"IN"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "the set should not be complete");
    assert_eq!(rq.get_num_bytes(), 2, "num bytes mismatch");

    let mut buf = vec![0u8; 16];
    let result = rq.read(&mut buf);
    assert!(result.is_err(), "read() should not succeed");
    assert_eq!(rq.get_num_bytes(), 2, "num bytes mismatch");

    Ok(())
}

#[test]
fn test_reassembly_queue_should_fail_to_read_if_the_nex_ssn_is_not_ready() -> Result<()> {
    let mut rq = ReassemblyQueue::new(0);

    let org_ppi = PayloadProtocolIdentifier::Binary;

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 123,
        stream_sequence_number: 1,
        user_data: Bytes::from_static(b"IN"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(complete, "the set should be complete");
    assert_eq!(rq.get_num_bytes(), 2, "num bytes mismatch");

    let mut buf = vec![0u8; 16];
    let result = rq.read(&mut buf);
    assert!(result.is_err(), "read() should not succeed");
    assert_eq!(rq.get_num_bytes(), 2, "num bytes mismatch");

    Ok(())
}

#[test]
fn test_reassembly_queue_detect_buffer_too_short() -> Result<()> {
    let mut rq = ReassemblyQueue::new(0);

    let org_ppi = PayloadProtocolIdentifier::Binary;

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 123,
        stream_sequence_number: 0,
        user_data: Bytes::from_static(b"0123456789"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(complete, "the set should be complete");
    assert_eq!(rq.get_num_bytes(), 10, "num bytes mismatch");

    let mut buf = vec![0u8; 8]; // <- passing buffer too short
    let result = rq.read(&mut buf);
    assert!(result.is_err(), "read() should not succeed");
    if let Err(err) = result {
        assert_eq!(
            err,
            Error::ErrShortBuffer { size: 8 },
            "read() should not succeed"
        );
    }
    assert_eq!(rq.get_num_bytes(), 0, "num bytes mismatch");

    Ok(())
}

#[test]
fn test_reassembly_queue_forward_tsn_for_ordered_fragments() -> Result<()> {
    let mut rq = ReassemblyQueue::new(0);

    let org_ppi = PayloadProtocolIdentifier::Binary;

    let ssn_complete = 5u16;
    let ssn_dropped = 6u16;

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 10,
        stream_sequence_number: ssn_complete,
        user_data: Bytes::from_static(b"123"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(complete, "chunk set should be complete");
    assert_eq!(rq.get_num_bytes(), 3, "num bytes mismatch");

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        beginning_fragment: true,
        tsn: 11,
        stream_sequence_number: ssn_dropped,
        user_data: Bytes::from_static(b"ABC"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk set should not be complete yet");
    assert_eq!(rq.get_num_bytes(), 6, "num bytes mismatch");

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        tsn: 12,
        stream_sequence_number: ssn_dropped,
        user_data: Bytes::from_static(b"DEF"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk set should not be complete yet");
    assert_eq!(rq.get_num_bytes(), 9, "num bytes mismatch");

    rq.forward_tsn_for_ordered(ssn_dropped);

    assert_eq!(rq.ordered.len(), 1, "there should be one chunk left");
    assert_eq!(rq.get_num_bytes(), 3, "num bytes mismatch");

    Ok(())
}

#[test]
fn test_reassembly_queue_forward_tsn_for_unordered_fragments() -> Result<()> {
    let mut rq = ReassemblyQueue::new(0);

    let org_ppi = PayloadProtocolIdentifier::Binary;

    let ssn_dropped = 6u16;
    let ssn_kept = 7u16;

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        unordered: true,
        beginning_fragment: true,
        tsn: 11,
        stream_sequence_number: ssn_dropped,
        user_data: Bytes::from_static(b"ABC"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk set should not be complete yet");
    assert_eq!(rq.get_num_bytes(), 3, "num bytes mismatch");

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        unordered: true,
        tsn: 12,
        stream_sequence_number: ssn_dropped,
        user_data: Bytes::from_static(b"DEF"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk set should not be complete yet");
    assert_eq!(rq.get_num_bytes(), 6, "num bytes mismatch");

    let chunk = ChunkPayloadData {
        payload_type: org_ppi,
        unordered: true,
        tsn: 14,
        beginning_fragment: true,
        stream_sequence_number: ssn_kept,
        user_data: Bytes::from_static(b"SOS"),
        ..Default::default()
    };

    let complete = rq.push(chunk);
    assert!(!complete, "chunk set should not be complete yet");
    assert_eq!(rq.get_num_bytes(), 9, "num bytes mismatch");

    // At this point, there are 3 chunks in the rq.unorderedChunks.
    // This call should remove chunks with tsn equals to 13 or older.
    rq.forward_tsn_for_unordered(13);

    // As a result, there should be one chunk (tsn=14)
    assert_eq!(
        rq.unordered_chunks.len(),
        1,
        "there should be one chunk kept"
    );
    assert_eq!(rq.get_num_bytes(), 3, "num bytes mismatch");

    Ok(())
}

#[test]
fn test_chunk_set_empty_chunk_set() -> Result<()> {
    let cset = ChunkSet::new(0, PayloadProtocolIdentifier::default());
    assert!(!cset.is_complete(), "empty chunkSet cannot be complete");
    Ok(())
}

#[test]
fn test_chunk_set_push_dup_chunks_to_chunk_set() -> Result<()> {
    let mut cset = ChunkSet::new(0, PayloadProtocolIdentifier::default());
    cset.push(ChunkPayloadData {
        tsn: 100,
        beginning_fragment: true,
        ..Default::default()
    });
    let complete = cset.push(ChunkPayloadData {
        tsn: 100,
        ending_fragment: true,
        ..Default::default()
    });
    assert!(!complete, "chunk with dup TSN is not complete");
    assert_eq!(cset.chunks.len(), 1, "chunk with dup TSN should be ignored");
    Ok(())
}

#[test]
fn test_chunk_set_incomplete_chunk_set_no_beginning() -> Result<()> {
    let cset = ChunkSet {
        ssn: 0,
        ppi: PayloadProtocolIdentifier::default(),
        chunks: vec![],
    };
    assert!(
        !cset.is_complete(),
        "chunkSet not starting with B=1 cannot be complete"
    );
    Ok(())
}

#[test]
fn test_chunk_set_incomplete_chunk_set_no_contiguous_tsn() -> Result<()> {
    let cset = ChunkSet {
        ssn: 0,
        ppi: PayloadProtocolIdentifier::default(),
        chunks: vec![
            ChunkPayloadData {
                tsn: 100,
                beginning_fragment: true,
                ..Default::default()
            },
            ChunkPayloadData {
                tsn: 101,
                ..Default::default()
            },
            ChunkPayloadData {
                tsn: 103,
                ending_fragment: true,
                ..Default::default()
            },
        ],
    };
    assert!(
        !cset.is_complete(),
        "chunkSet not starting with incontiguous tsn cannot be complete"
    );
    Ok(())
}
