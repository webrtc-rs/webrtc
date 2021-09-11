use super::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

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
