use crate::error::Error;

///////////////////////////////////////////////////////////////////
//payload_queue_test
///////////////////////////////////////////////////////////////////
use super::payload_queue::*;
use crate::chunk::chunk_payload_data::ChunkPayloadData;
use crate::chunk::chunk_selective_ack::GapAckBlock;
use bytes::BytesMut;

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
fn test_payload_queue_push_no_check() -> Result<(), Error> {
    let mut pq = PayloadQueue::new();

    pq.push_no_check(make_payload(0, 10));
    assert_eq!(10, pq.get_num_bytes(), "total bytes mismatch");
    assert_eq!(1, pq.len(), "item count mismatch");
    pq.push_no_check(make_payload(1, 11));
    assert_eq!(21, pq.get_num_bytes(), "total bytes mismatch");
    assert_eq!(2, pq.len(), "item count mismatch");
    pq.push_no_check(make_payload(2, 12));
    assert_eq!(33, pq.get_num_bytes(), "total bytes mismatch");
    assert_eq!(3, pq.len(), "item count mismatch");

    for i in 0..3 {
        assert!(!pq.sorted.is_empty(), "should not be empty");
        let c = pq.pop(i);
        assert!(c.is_some(), "pop should succeed");
        if let Some(c) = c {
            assert_eq!(i, c.tsn, "TSN should match");
        }
    }

    assert_eq!(0, pq.get_num_bytes(), "total bytes mismatch");
    assert_eq!(0, pq.len(), "item count mismatch");

    assert!(pq.sorted.is_empty(), "should be empty");
    pq.push_no_check(make_payload(3, 13));
    assert_eq!(13, pq.get_num_bytes(), "total bytes mismatch");
    pq.push_no_check(make_payload(4, 14));
    assert_eq!(27, pq.get_num_bytes(), "total bytes mismatch");

    for i in 3..5 {
        assert!(!pq.sorted.is_empty(), "should not be empty");
        let c = pq.pop(i);
        assert!(c.is_some(), "pop should succeed");
        if let Some(c) = c {
            assert_eq!(i, c.tsn, "TSN should match");
        }
    }

    assert_eq!(0, pq.get_num_bytes(), "total bytes mismatch");
    assert_eq!(0, pq.len(), "item count mismatch");

    Ok(())
}

#[test]
fn test_payload_queue_get_gap_ack_block() -> Result<(), Error> {
    let mut pq = PayloadQueue::new();

    pq.push(make_payload(1, 0), 0);
    pq.push(make_payload(2, 0), 0);
    pq.push(make_payload(3, 0), 0);
    pq.push(make_payload(4, 0), 0);
    pq.push(make_payload(5, 0), 0);
    pq.push(make_payload(6, 0), 0);

    let gab1 = vec![GapAckBlock { start: 1, end: 6 }];
    let gab2 = pq.get_gap_ack_blocks(0);
    assert!(!gab2.is_empty());
    assert_eq!(gab2.len(), 1);

    assert_eq!(gab1[0].start, gab2[0].start);
    assert_eq!(gab1[0].end, gab2[0].end);

    pq.push(make_payload(8, 0), 0);
    pq.push(make_payload(9, 0), 0);

    let gab1 = vec![
        GapAckBlock { start: 1, end: 6 },
        GapAckBlock { start: 8, end: 9 },
    ];
    let gab2 = pq.get_gap_ack_blocks(0);
    assert!(!gab2.is_empty());
    assert_eq!(gab2.len(), 2);

    assert_eq!(gab1[0].start, gab2[0].start);
    assert_eq!(gab1[0].end, gab2[0].end);
    assert_eq!(gab1[1].start, gab2[1].start);
    assert_eq!(gab1[1].end, gab2[1].end);

    Ok(())
}

#[test]
fn test_payload_queue_get_last_tsn_received() -> Result<(), Error> {
    let mut pq = PayloadQueue::new();

    // empty queie should return false
    let ok = pq.get_last_tsn_received();
    assert!(ok.is_none(), "should be none");

    let ok = pq.push(make_payload(20, 0), 0);
    assert!(ok, "should be true");
    let tsn = pq.get_last_tsn_received();
    assert!(tsn.is_some(), "should be false");
    assert_eq!(Some(&20), tsn, "should match");

    // append should work
    let ok = pq.push(make_payload(21, 0), 0);
    assert!(ok, "should be true");
    let tsn = pq.get_last_tsn_received();
    assert!(tsn.is_some(), "should be false");
    assert_eq!(Some(&21), tsn, "should match");

    // check if sorting applied
    let ok = pq.push(make_payload(19, 0), 0);
    assert!(ok, "should be true");
    let tsn = pq.get_last_tsn_received();
    assert!(tsn.is_some(), "should be false");
    assert_eq!(Some(&21), tsn, "should match");

    Ok(())
}

#[test]
fn test_payload_queue_mark_all_to_retrasmit() -> Result<(), Error> {
    let mut pq = PayloadQueue::new();

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
fn test_payload_queue_reset_retransmit_flag_on_ack() -> Result<(), Error> {
    let mut pq = PayloadQueue::new();

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
