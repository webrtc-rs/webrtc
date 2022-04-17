use super::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

#[test]
fn test_stream_buffered_amount() -> Result<()> {
    let s = Stream::default();

    assert_eq!(0, s.buffered_amount());
    assert_eq!(0, s.buffered_amount_low_threshold());

    s.buffered_amount.store(8192, Ordering::SeqCst);
    s.set_buffered_amount_low_threshold(2048);
    assert_eq!(8192, s.buffered_amount(), "unexpected bufferedAmount");
    assert_eq!(
        2048,
        s.buffered_amount_low_threshold(),
        "unexpected threshold"
    );

    Ok(())
}

#[tokio::test]
async fn test_stream_amount_on_buffered_amount_low() -> Result<()> {
    let s = Stream::default();

    s.buffered_amount.store(4096, Ordering::SeqCst);
    s.set_buffered_amount_low_threshold(2048);

    let n_cbs = Arc::new(AtomicU32::new(0));
    let n_cbs2 = n_cbs.clone();

    s.on_buffered_amount_low(Box::new(move || {
        n_cbs2.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }))
    .await;

    // Negative value should be ignored (by design)
    s.on_buffer_released(-32).await; // bufferedAmount = 3072
    assert_eq!(4096, s.buffered_amount(), "unexpected bufferedAmount");
    assert_eq!(0, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

    // Above to above, no callback
    s.on_buffer_released(1024).await; // bufferedAmount = 3072
    assert_eq!(3072, s.buffered_amount(), "unexpected bufferedAmount");
    assert_eq!(0, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

    // Above to equal, callback should be made
    s.on_buffer_released(1024).await; // bufferedAmount = 2048
    assert_eq!(2048, s.buffered_amount(), "unexpected bufferedAmount");
    assert_eq!(1, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

    // Eaual to below, no callback
    s.on_buffer_released(1024).await; // bufferedAmount = 1024
    assert_eq!(1024, s.buffered_amount(), "unexpected bufferedAmount");
    assert_eq!(1, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

    // Blow to below, no callback
    s.on_buffer_released(1024).await; // bufferedAmount = 0
    assert_eq!(0, s.buffered_amount(), "unexpected bufferedAmount");
    assert_eq!(1, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

    // Capped at 0, no callback
    s.on_buffer_released(1024).await; // bufferedAmount = 0
    assert_eq!(0, s.buffered_amount(), "unexpected bufferedAmount");
    assert_eq!(1, n_cbs.load(Ordering::SeqCst), "callback count mismatch");

    Ok(())
}

#[tokio::test]
async fn test_poll_stream() -> std::result::Result<(), io::Error> {
    let s = Arc::new(Stream::new(
        "test_poll_stream".to_owned(),
        0,
        4096,
        Arc::new(AtomicU32::new(4096)),
        Arc::new(AtomicU8::new(AssociationState::Established as u8)),
        None,
        Arc::new(PendingQueue::new()),
    ));
    let mut poll_stream = PollStream::new(s.clone());

    // getters
    assert_eq!(0, poll_stream.stream_identifier());
    assert_eq!(0, poll_stream.buffered_amount());
    assert_eq!(0, poll_stream.buffered_amount_low_threshold());
    assert_eq!(0, poll_stream.get_num_bytes_in_reassembly_queue().await);

    // async write
    let n = poll_stream.write(&[1, 2, 3]).await?;
    assert_eq!(3, n);
    assert_eq!(3, poll_stream.buffered_amount());

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
    poll_stream.read(&mut buf).await?;
    assert_eq!(buf, [0, 1, 2, 3, 4]);

    // shutdown
    poll_stream.shutdown().await?;
    assert_eq!(true, sc.closed.load(Ordering::Relaxed));
    assert!(poll_stream.read(&mut buf).await.is_err());

    // misc.
    let clone = poll_stream.clone();
    assert_eq!(clone.stream_identifier(), poll_stream.stream_identifier());

    Ok(())
}
