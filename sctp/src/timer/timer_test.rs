// Silence warning on `for i in 0..vec.len() { … }`:
#![allow(clippy::needless_range_loop)]
// Silence warning on `..Default::default()` with no effect:
#![allow(clippy::needless_update)]

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

///////////////////////////////////////////////////////////////////
//ack_timer_test
///////////////////////////////////////////////////////////////////
use super::ack_timer::*;

mod test_ack_timer {
    use crate::error::Result;

    use super::*;

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
    async fn test_ack_timer_start_and_stop() -> Result<()> {
        let ncbs = Arc::new(AtomicU32::new(0));
        let obs = Arc::new(Mutex::new(TestAckTimerObserver { ncbs: ncbs.clone() }));

        let mut rt = AckTimer::new(Arc::downgrade(&obs), ACK_INTERVAL);

        // should start ok
        let ok = rt.start();
        assert!(ok, "start() should succeed");
        assert!(rt.is_running(), "should be running");

        // stop immedidately
        rt.stop();
        assert!(!rt.is_running(), "should not be running");

        // Sleep more than 200msec of interval to test if it never times out
        sleep(ACK_INTERVAL + Duration::from_millis(50)).await;

        assert_eq!(
            ncbs.load(Ordering::SeqCst),
            0,
            "should not be timed out (actual: {})",
            ncbs.load(Ordering::SeqCst)
        );

        // can start again
        let ok = rt.start();
        assert!(ok, "start() should succeed again");
        assert!(rt.is_running(), "should be running");

        // should close ok
        rt.stop();
        assert!(!rt.is_running(), "should not be running");

        Ok(())
    }
}

///////////////////////////////////////////////////////////////////
//rtx_timer_test
///////////////////////////////////////////////////////////////////
use super::rtx_timer::*;

mod test_rto_manager {
    use crate::error::Result;

    use super::*;

    #[tokio::test]
    async fn test_rto_manager_initial_values() -> Result<()> {
        let m = RtoManager::new();
        assert_eq!(m.rto, RTO_INITIAL, "should be rtoInitial");
        assert_eq!(m.get_rto(), RTO_INITIAL, "should be rtoInitial");
        assert_eq!(m.srtt, 0, "should be 0");
        assert_eq!(m.rttvar, 0.0, "should be 0.0");

        Ok(())
    }

    #[tokio::test]
    async fn test_rto_manager_rto_calculation_small_rtt() -> Result<()> {
        let mut m = RtoManager::new();
        let exp = vec![
            1800, 1500, 1275, 1106, 1000, // capped at RTO.Min
        ];

        for i in 0..5 {
            m.set_new_rtt(600);
            let rto = m.get_rto();
            assert_eq!(rto, exp[i], "should be equal: {i}");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_rto_manager_rto_calculation_large_rtt() -> Result<()> {
        let mut m = RtoManager::new();
        let exp = vec![
            60000, // capped at RTO.Max
            60000, // capped at RTO.Max
            60000, // capped at RTO.Max
            55312, 48984,
        ];

        for i in 0..5 {
            m.set_new_rtt(30000);
            let rto = m.get_rto();
            assert_eq!(rto, exp[i], "should be equal: {i}");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_rto_manager_calculate_next_timeout() -> Result<()> {
        let rto = calculate_next_timeout(1, 0);
        assert_eq!(rto, 1, "should match");
        let rto = calculate_next_timeout(1, 1);
        assert_eq!(rto, 2, "should match");
        let rto = calculate_next_timeout(1, 2);
        assert_eq!(rto, 4, "should match");
        let rto = calculate_next_timeout(1, 30);
        assert_eq!(rto, 60000, "should match");
        let rto = calculate_next_timeout(1, 63);
        assert_eq!(rto, 60000, "should match");
        let rto = calculate_next_timeout(1, 64);
        assert_eq!(rto, 60000, "should match");

        Ok(())
    }

    #[tokio::test]
    async fn test_rto_manager_reset() -> Result<()> {
        let mut m = RtoManager::new();
        for _ in 0..10 {
            m.set_new_rtt(200);
        }

        m.reset();
        assert_eq!(m.get_rto(), RTO_INITIAL, "should be rtoInitial");
        assert_eq!(m.srtt, 0, "should be 0");
        assert_eq!(m.rttvar, 0.0, "should be 0");

        Ok(())
    }
}

//TODO: remove this conditional test
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod test_rtx_timer {
    use super::*;
    use crate::association::RtxTimerId;
    use crate::error::Result;

    use std::time::SystemTime;
    use tokio::sync::mpsc;

    struct TestTimerObserver {
        ncbs: Arc<AtomicU32>,
        timer_id: RtxTimerId,
        done_tx: Option<mpsc::Sender<SystemTime>>,
        max_rtos: usize,
    }

    impl Default for TestTimerObserver {
        fn default() -> Self {
            TestTimerObserver {
                ncbs: Arc::new(AtomicU32::new(0)),
                timer_id: RtxTimerId::T1Init,
                done_tx: None,
                max_rtos: 0,
            }
        }
    }

    #[async_trait]
    impl RtxTimerObserver for TestTimerObserver {
        async fn on_retransmission_timeout(&mut self, timer_id: RtxTimerId, n_rtos: usize) {
            self.ncbs.fetch_add(1, Ordering::SeqCst);
            // 30 : 1 (30)
            // 60 : 2 (90)
            // 120: 3 (210)
            // 240: 4 (550) <== expected in 650 msec
            assert_eq!(self.timer_id, timer_id, "unexpected timer ID: {timer_id}");
            if (self.max_rtos > 0 && n_rtos == self.max_rtos) || self.max_rtos == usize::MAX {
                if let Some(done) = &self.done_tx {
                    let elapsed = SystemTime::now();
                    let _ = done.send(elapsed).await;
                }
            }
        }

        async fn on_retransmission_failure(&mut self, timer_id: RtxTimerId) {
            if self.max_rtos == 0 {
                if let Some(done) = &self.done_tx {
                    assert_eq!(self.timer_id, timer_id, "unexpted timer ID: {timer_id}");
                    let elapsed = SystemTime::now();
                    //t.Logf("onRtxFailure: elapsed=%.03f\n", elapsed)
                    let _ = done.send(elapsed).await;
                }
            } else {
                panic!("timer should not fail");
            }
        }
    }

    #[tokio::test]
    async fn test_rtx_timer_callback_interval() -> Result<()> {
        let timer_id = RtxTimerId::T1Init;
        let ncbs = Arc::new(AtomicU32::new(0));
        let obs = Arc::new(Mutex::new(TestTimerObserver {
            ncbs: ncbs.clone(),
            timer_id,
            ..Default::default()
        }));
        let rt = RtxTimer::new(Arc::downgrade(&obs), timer_id, PATH_MAX_RETRANS);

        assert!(!rt.is_running().await, "should not be running");

        // since := time.Now()
        let ok = rt.start(30).await;
        assert!(ok, "should be true");
        assert!(rt.is_running().await, "should be running");

        sleep(Duration::from_millis(650)).await;
        rt.stop().await;
        assert!(!rt.is_running().await, "should not be running");

        assert_eq!(ncbs.load(Ordering::SeqCst), 4, "should be called 4 times");

        Ok(())
    }

    #[tokio::test]
    async fn test_rtx_timer_last_start_wins() -> Result<()> {
        let timer_id = RtxTimerId::T3RTX;
        let ncbs = Arc::new(AtomicU32::new(0));
        let obs = Arc::new(Mutex::new(TestTimerObserver {
            ncbs: ncbs.clone(),
            timer_id,
            ..Default::default()
        }));
        let rt = RtxTimer::new(Arc::downgrade(&obs), timer_id, PATH_MAX_RETRANS);

        let interval = 30;
        let ok = rt.start(interval).await;
        assert!(ok, "should be accepted");
        let ok = rt.start(interval * 99).await; // should ignored
        assert!(!ok, "should be ignored");
        let ok = rt.start(interval * 99).await; // should ignored
        assert!(!ok, "should be ignored");

        sleep(Duration::from_millis((interval * 3) / 2)).await;
        rt.stop().await;

        assert!(!rt.is_running().await, "should not be running");
        assert_eq!(ncbs.load(Ordering::SeqCst), 1, "must be called once");

        Ok(())
    }

    #[tokio::test]
    async fn test_rtx_timer_stop_right_after_start() -> Result<()> {
        let timer_id = RtxTimerId::T3RTX;
        let ncbs = Arc::new(AtomicU32::new(0));
        let obs = Arc::new(Mutex::new(TestTimerObserver {
            ncbs: ncbs.clone(),
            timer_id,
            ..Default::default()
        }));
        let rt = RtxTimer::new(Arc::downgrade(&obs), timer_id, PATH_MAX_RETRANS);

        let interval = 30;
        let ok = rt.start(interval).await;
        assert!(ok, "should be accepted");
        rt.stop().await;

        sleep(Duration::from_millis((interval * 3) / 2)).await;
        rt.stop().await;

        assert!(!rt.is_running().await, "should not be running");
        assert_eq!(ncbs.load(Ordering::SeqCst), 0, "no callback should be made");

        Ok(())
    }

    #[tokio::test]
    async fn test_rtx_timer_start_stop_then_start() -> Result<()> {
        let timer_id = RtxTimerId::T1Cookie;
        let ncbs = Arc::new(AtomicU32::new(0));
        let obs = Arc::new(Mutex::new(TestTimerObserver {
            ncbs: ncbs.clone(),
            timer_id,
            ..Default::default()
        }));
        let rt = RtxTimer::new(Arc::downgrade(&obs), timer_id, PATH_MAX_RETRANS);

        let interval = 30;
        let ok = rt.start(interval).await;
        assert!(ok, "should be accepted");
        rt.stop().await;
        assert!(!rt.is_running().await, "should NOT be running");
        let ok = rt.start(interval).await;
        assert!(ok, "should be accepted");
        assert!(rt.is_running().await, "should be running");

        sleep(Duration::from_millis((interval * 3) / 2)).await;
        rt.stop().await;

        assert!(!rt.is_running().await, "should NOT be running");
        assert_eq!(ncbs.load(Ordering::SeqCst), 1, "must be called once");

        Ok(())
    }

    #[tokio::test]
    async fn test_rtx_timer_start_and_stop_in_atight_loop() -> Result<()> {
        let timer_id = RtxTimerId::T2Shutdown;
        let ncbs = Arc::new(AtomicU32::new(0));
        let obs = Arc::new(Mutex::new(TestTimerObserver {
            ncbs: ncbs.clone(),
            timer_id,
            ..Default::default()
        }));
        let rt = RtxTimer::new(Arc::downgrade(&obs), timer_id, PATH_MAX_RETRANS);

        for _ in 0..1000 {
            let ok = rt.start(30).await;
            assert!(ok, "should be accepted");
            assert!(rt.is_running().await, "should be running");
            rt.stop().await;
            assert!(!rt.is_running().await, "should NOT be running");
        }

        assert_eq!(ncbs.load(Ordering::SeqCst), 0, "no callback should be made");

        Ok(())
    }

    #[tokio::test]
    async fn test_rtx_timer_should_stop_after_rtx_failure() -> Result<()> {
        let (done_tx, mut done_rx) = mpsc::channel(1);

        let timer_id = RtxTimerId::Reconfig;
        let ncbs = Arc::new(AtomicU32::new(0));
        let obs = Arc::new(Mutex::new(TestTimerObserver {
            ncbs: ncbs.clone(),
            timer_id,
            done_tx: Some(done_tx),
            ..Default::default()
        }));

        let since = SystemTime::now();
        let rt = RtxTimer::new(Arc::downgrade(&obs), timer_id, PATH_MAX_RETRANS);

        // RTO(msec) Total(msec)
        //  10          10    1st RTO
        //  20          30    2nd RTO
        //  40          70    3rd RTO
        //  80         150    4th RTO
        // 160         310    5th RTO (== Path.Max.Retrans)
        // 320         630    Failure

        let interval = 10;
        let ok = rt.start(interval).await;
        assert!(ok, "should be accepted");
        assert!(rt.is_running().await, "should be running");

        let elapsed = done_rx.recv().await;

        assert!(!rt.is_running().await, "should not be running");
        assert_eq!(ncbs.load(Ordering::SeqCst), 5, "should be called 5 times");

        if let Some(elapsed) = elapsed {
            let diff = elapsed.duration_since(since).unwrap();
            assert!(
                diff > Duration::from_millis(600),
                "must have taken more than 600 msec"
            );
            assert!(
                diff < Duration::from_millis(700),
                "must fail in less than 700 msec"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_rtx_timer_should_not_stop_if_max_retrans_is_zero() -> Result<()> {
        let (done_tx, mut done_rx) = mpsc::channel(1);

        let timer_id = RtxTimerId::Reconfig;
        let max_rtos = 6;
        let ncbs = Arc::new(AtomicU32::new(0));
        let obs = Arc::new(Mutex::new(TestTimerObserver {
            ncbs: ncbs.clone(),
            timer_id,
            done_tx: Some(done_tx),
            max_rtos,
            ..Default::default()
        }));

        let since = SystemTime::now();
        let rt = RtxTimer::new(Arc::downgrade(&obs), timer_id, 0);

        // RTO(msec) Total(msec)
        //  10          10    1st RTO
        //  20          30    2nd RTO
        //  40          70    3rd RTO
        //  80         150    4th RTO
        // 160         310    5th RTO
        // 320         630    6th RTO => exit test (timer should still be running)

        let interval = 10;
        let ok = rt.start(interval).await;
        assert!(ok, "should be accepted");
        assert!(rt.is_running().await, "should be running");

        let elapsed = done_rx.recv().await;

        assert!(rt.is_running().await, "should still be running");
        assert_eq!(ncbs.load(Ordering::SeqCst), 6, "should be called 6 times");

        if let Some(elapsed) = elapsed {
            let diff = elapsed.duration_since(since).unwrap();
            assert!(
                diff > Duration::from_millis(600),
                "must have taken more than 600 msec"
            );
            assert!(
                diff < Duration::from_millis(700),
                "must fail in less than 700 msec"
            );
        }

        rt.stop().await;

        Ok(())
    }

    #[tokio::test]
    async fn test_rtx_timer_stop_timer_that_is_not_running_is_noop() -> Result<()> {
        let (done_tx, mut done_rx) = mpsc::channel(1);

        let timer_id = RtxTimerId::Reconfig;
        let obs = Arc::new(Mutex::new(TestTimerObserver {
            timer_id,
            done_tx: Some(done_tx),
            max_rtos: usize::MAX,
            ..Default::default()
        }));
        let rt = RtxTimer::new(Arc::downgrade(&obs), timer_id, PATH_MAX_RETRANS);

        for _ in 0..10 {
            rt.stop().await;
        }

        let ok = rt.start(20).await;
        assert!(ok, "should be accepted");
        assert!(rt.is_running().await, "must be running");

        let _ = done_rx.recv().await;
        rt.stop().await;
        assert!(!rt.is_running().await, "must be false");

        Ok(())
    }

    #[tokio::test]
    async fn test_rtx_timer_closed_timer_wont_start() -> Result<()> {
        let timer_id = RtxTimerId::Reconfig;
        let ncbs = Arc::new(AtomicU32::new(0));
        let obs = Arc::new(Mutex::new(TestTimerObserver {
            ncbs: ncbs.clone(),
            timer_id,
            ..Default::default()
        }));
        let rt = RtxTimer::new(Arc::downgrade(&obs), timer_id, PATH_MAX_RETRANS);

        let ok = rt.start(20).await;
        assert!(ok, "should be accepted");
        assert!(rt.is_running().await, "must be running");

        rt.stop().await;
        assert!(!rt.is_running().await, "must be false");

        //let ok = rt.start(obs.clone(), 20).await;
        //assert!(!ok, "should not start");
        assert!(!rt.is_running().await, "must not be running");

        sleep(Duration::from_millis(100)).await;
        assert_eq!(ncbs.load(Ordering::SeqCst), 0, "RTO should not occur");

        Ok(())
    }
}
