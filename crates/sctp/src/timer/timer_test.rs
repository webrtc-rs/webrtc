use crate::error::Error;

///////////////////////////////////////////////////////////////////
//ack_timer_test
///////////////////////////////////////////////////////////////////
use super::ack_timer::*;

use async_trait::async_trait;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

struct TestAckTimerObserver {
    ncbs: Arc<AtomicU32>,
}

#[async_trait]
impl AckTimerObserver for TestAckTimerObserver {
    async fn on_ack_timeout(&mut self) {
        log::trace!("ack timed out");
        self.ncbs.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn test_ack_timer_start_and_stop() -> Result<(), Error> {
    let mut rt = AckTimer::new(ACK_INTERVAL);

    let ncbs = Arc::new(AtomicU32::new(0));
    let obs = Arc::new(Mutex::new(TestAckTimerObserver { ncbs: ncbs.clone() }));

    // should start ok
    let ok = rt.start(obs.clone());
    assert!(ok, "start() should succeed");
    assert!(rt.is_running(), "should be running");

    // stop immedidately
    rt.stop();
    assert!(!rt.is_running(), "should not be running");

    // Sleep more than 200msec of interval to test if it never times out
    sleep(ACK_INTERVAL + Duration::from_millis(50)).await;

    assert_eq!(
        0,
        ncbs.load(Ordering::SeqCst),
        "should not be timed out (actual: {})",
        ncbs.load(Ordering::SeqCst)
    );

    // can start again
    let ok = rt.start(obs);
    assert!(ok, "start() should succeed again");
    assert!(rt.is_running(), "should be running");

    // should close ok
    rt.stop();
    assert!(!rt.is_running(), "should not be running");

    Ok(())
}

///////////////////////////////////////////////////////////////////
//rtx_timer_test
///////////////////////////////////////////////////////////////////
