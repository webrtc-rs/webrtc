use super::*;
use crate::error::Result;

struct DummyPeriodicTimerTimeoutHandler;

#[async_trait]
impl PeriodicTimerTimeoutHandler for DummyPeriodicTimerTimeoutHandler {
    async fn on_timeout(&mut self, id: TimerIdRefresh) {
        assert_eq!(id, TimerIdRefresh::Perms);
    }
}

#[tokio::test]
async fn test_periodic_timer() -> Result<()> {
    let timer_id = TimerIdRefresh::Perms;
    let rt = PeriodicTimer::new(timer_id, Duration::from_millis(50));
    let dummy1 = Arc::new(Mutex::new(DummyPeriodicTimerTimeoutHandler {}));
    let dummy2 = Arc::clone(&dummy1);

    assert!(!rt.is_running().await, "should not be running yet");

    let ok = rt.start(dummy1).await;
    assert!(ok, "should be true");
    assert!(rt.is_running().await, "should be running");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let ok = rt.start(dummy2).await;
    assert!(!ok, "start again is noop");

    tokio::time::sleep(Duration::from_millis(120)).await;
    rt.stop().await;

    assert!(!rt.is_running().await, "should not be running");

    Ok(())
}
