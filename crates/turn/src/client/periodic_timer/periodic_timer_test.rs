use super::*;

use util::Error;

fn timeout_handler(id: TimerIdRefresh) {
    assert_eq!(id, TimerIdRefresh::Perms);
}

#[tokio::test]
async fn test_periodic_timer() -> Result<(), Error> {
    let timer_id = TimerIdRefresh::Perms;
    let mut rt = PeriodicTimer::new(timer_id, Some(timeout_handler), Duration::from_millis(50));

    assert!(!rt.is_running(), "should not be running yet");

    let ok = rt.start();
    assert!(ok, "should be true");
    assert!(rt.is_running(), "should be running");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let ok = rt.start();
    assert!(!ok, "start again is noop");

    tokio::time::sleep(Duration::from_millis(120)).await;
    rt.stop();

    assert!(!rt.is_running(), "should not be running");

    Ok(())
}
