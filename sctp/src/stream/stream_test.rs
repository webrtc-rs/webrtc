use std::sync::atomic::Ordering;
use std::sync::Arc;

use portable_atomic::AtomicU32;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;

fn create_test_stream() -> Stream {
    let (awake_write_loop_ch_tx, _awake_write_loop_ch_rx) = mpsc::channel(1);
    Stream::new(
        "client".to_owned(),
        0,
        0,
        Arc::new(AtomicU32::default()),
        Arc::new(AtomicU8::default()),
        Arc::new(awake_write_loop_ch_tx),
        Arc::new(PendingQueue::default()),
    )
}

#[test]
fn test_stream_buffered_amount() -> Result<()> {
    let s = create_test_stream();

    assert_eq!(s.buffered_amount(), 0);
    assert_eq!(s.buffered_amount_low_threshold(), 0);

    s.buffered_amount.store(8192, Ordering::SeqCst);
    s.set_buffered_amount_low_threshold(2048);
    assert_eq!(s.buffered_amount(), 8192, "unexpected bufferedAmount");
    assert_eq!(
        s.buffered_amount_low_threshold(),
        2048,
        "unexpected threshold"
    );

    Ok(())
}

#[tokio::test]
async fn test_stream_amount_on_buffered_amount_low() -> Result<()> {
    let s = create_test_stream();

    s.buffered_amount.store(4096, Ordering::SeqCst);
    s.set_buffered_amount_low_threshold(2048);

    let n_cbs = Arc::new(AtomicU32::new(0));
    let n_cbs2 = n_cbs.clone();

    s.on_buffered_amount_low(Box::new(move || {
        n_cbs2.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }));

    // Negative value should be ignored (by design)
    s.on_buffer_released(-32).await; // bufferedAmount = 3072
    assert_eq!(s.buffered_amount(), 4096, "unexpected bufferedAmount");
    assert_eq!(n_cbs.load(Ordering::SeqCst), 0, "callback count mismatch");

    // Above to above, no callback
    s.on_buffer_released(1024).await; // bufferedAmount = 3072
    assert_eq!(s.buffered_amount(), 3072, "unexpected bufferedAmount");
    assert_eq!(n_cbs.load(Ordering::SeqCst), 0, "callback count mismatch");

    // Above to equal, callback should be made
    s.on_buffer_released(1024).await; // bufferedAmount = 2048
    assert_eq!(s.buffered_amount(), 2048, "unexpected bufferedAmount");
    assert_eq!(n_cbs.load(Ordering::SeqCst), 1, "callback count mismatch");

    // Eaual to below, no callback
    s.on_buffer_released(1024).await; // bufferedAmount = 1024
    assert_eq!(s.buffered_amount(), 1024, "unexpected bufferedAmount");
    assert_eq!(n_cbs.load(Ordering::SeqCst), 1, "callback count mismatch");

    // Blow to below, no callback
    s.on_buffer_released(1024).await; // bufferedAmount = 0
    assert_eq!(s.buffered_amount(), 0, "unexpected bufferedAmount");
    assert_eq!(n_cbs.load(Ordering::SeqCst), 1, "callback count mismatch");

    // Capped at 0, no callback
    s.on_buffer_released(1024).await; // bufferedAmount = 0
    assert_eq!(s.buffered_amount(), 0, "unexpected bufferedAmount");
    assert_eq!(n_cbs.load(Ordering::SeqCst), 1, "callback count mismatch");

    Ok(())
}

#[tokio::test]
async fn test_stream() -> std::result::Result<(), io::Error> {
    let (awake_write_loop_ch_tx, _awake_write_loop_ch_rx) = mpsc::channel(1);
    let s = Stream::new(
        "test_poll_stream".to_owned(),
        0,
        4096,
        Arc::new(AtomicU32::new(4096)),
        Arc::new(AtomicU8::new(AssociationState::Established as u8)),
        Arc::new(awake_write_loop_ch_tx),
        Arc::new(PendingQueue::new()),
    );

    // getters
    assert_eq!(s.stream_identifier(), 0);
    assert_eq!(s.buffered_amount(), 0);
    assert_eq!(s.buffered_amount_low_threshold(), 0);
    assert_eq!(s.get_num_bytes_in_reassembly_queue().await, 0);

    // setters
    s.set_default_payload_type(PayloadProtocolIdentifier::Binary);
    s.set_reliability_params(true, ReliabilityType::Reliable, 0);

    // write
    let n = s.write(&Bytes::from("Hello ")).await?;
    assert_eq!(n, 6);
    assert_eq!(s.buffered_amount(), 6);
    let n = s
        .write_sctp(&Bytes::from("world"), PayloadProtocolIdentifier::Binary)
        .await?;
    assert_eq!(n, 5);
    assert_eq!(s.buffered_amount(), 11);

    // async read
    //  1. pretend that we've received a chunk
    s.handle_data(ChunkPayloadData {
        unordered: true,
        beginning_fragment: true,
        ending_fragment: true,
        user_data: Bytes::from_static(&[0, 1, 2, 3, 4]),
        payload_type: PayloadProtocolIdentifier::Binary,
        ..Default::default()
    })
    .await;
    //  2. read it
    let mut buf = [0; 5];
    s.read(&mut buf).await?;
    assert_eq!(buf, [0, 1, 2, 3, 4]);

    // shutdown write
    s.shutdown(Shutdown::Write).await?;
    // write must fail
    assert!(s.write(&Bytes::from("error")).await.is_err());
    // read should continue working
    s.handle_data(ChunkPayloadData {
        unordered: true,
        beginning_fragment: true,
        ending_fragment: true,
        user_data: Bytes::from_static(&[5, 6, 7, 8, 9]),
        payload_type: PayloadProtocolIdentifier::Binary,
        ..Default::default()
    })
    .await;
    let mut buf = [0; 5];
    s.read(&mut buf).await?;
    assert_eq!(buf, [5, 6, 7, 8, 9]);

    // shutdown read
    s.shutdown(Shutdown::Read).await?;
    // read must return 0
    assert_eq!(s.read(&mut buf).await, Ok(0));

    Ok(())
}

#[tokio::test]
async fn test_poll_stream() -> std::result::Result<(), io::Error> {
    let (awake_write_loop_ch_tx, _awake_write_loop_ch_rx) = mpsc::channel(1);
    let s = Arc::new(Stream::new(
        "test_poll_stream".to_owned(),
        0,
        4096,
        Arc::new(AtomicU32::new(4096)),
        Arc::new(AtomicU8::new(AssociationState::Established as u8)),
        Arc::new(awake_write_loop_ch_tx),
        Arc::new(PendingQueue::new()),
    ));
    let mut poll_stream = PollStream::new(s.clone());

    // getters
    assert_eq!(poll_stream.stream_identifier(), 0);
    assert_eq!(poll_stream.buffered_amount(), 0);
    assert_eq!(poll_stream.buffered_amount_low_threshold(), 0);
    assert_eq!(poll_stream.get_num_bytes_in_reassembly_queue().await, 0);

    // async write
    let n = poll_stream.write(&[1, 2, 3]).await?;
    assert_eq!(n, 3);
    poll_stream.flush().await?;
    assert_eq!(poll_stream.buffered_amount(), 3);

    // async read
    //  1. pretend that we've received a chunk
    let sc = s.clone();
    sc.handle_data(ChunkPayloadData {
        unordered: true,
        beginning_fragment: true,
        ending_fragment: true,
        user_data: Bytes::from_static(&[0, 1, 2, 3, 4]),
        payload_type: PayloadProtocolIdentifier::Binary,
        ..Default::default()
    })
    .await;
    //  2. read it
    let mut buf = [0; 5];
    poll_stream.read_exact(&mut buf).await?;
    assert_eq!(buf, [0, 1, 2, 3, 4]);

    // shutdown write
    poll_stream.shutdown().await?;
    // write must fail
    assert!(poll_stream.write(&[1, 2, 3]).await.is_err());
    // read should continue working
    sc.handle_data(ChunkPayloadData {
        unordered: true,
        beginning_fragment: true,
        ending_fragment: true,
        user_data: Bytes::from_static(&[5, 6, 7, 8, 9]),
        payload_type: PayloadProtocolIdentifier::Binary,
        ..Default::default()
    })
    .await;
    let mut buf = [0; 5];
    poll_stream.read_exact(&mut buf).await?;
    assert_eq!(buf, [5, 6, 7, 8, 9]);

    // misc.
    let clone = poll_stream.clone();
    assert_eq!(clone.stream_identifier(), poll_stream.stream_identifier());

    Ok(())
}
