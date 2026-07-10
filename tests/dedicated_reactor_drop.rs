//! Regression test: dropping a dedicated-reactor peer connection *without*
//! calling `close()` must still terminate its reactor OS thread (issue #101).
//!
//! This covers the shutdown-leak window raised in review: `Drop` sets the
//! `closing` flag and best-effort wakes the driver, so the reactor thread exits
//! even if the wake could not be enqueued — it does not leak. On Linux we prove
//! it directly by watching the OS thread named `webrtc-reactor` disappear from
//! `/proc/self/task`. Each file under `tests/` is its own test binary (own
//! process), so this thread count is not polluted by other tests.
use std::sync::Arc;

use webrtc::peer_connection::{PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::block_on;

#[cfg(target_os = "linux")]
use std::time::Duration;
#[cfg(target_os = "linux")]
use webrtc::runtime::sleep;

struct NoopHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for NoopHandler {}

#[test]
fn test_dedicated_reactor_thread_stops_on_drop() {
    block_on(run());
}

async fn run() {
    #[cfg(target_os = "linux")]
    let before = reactor_thread_count();

    let pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(NoopHandler))
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_dedicated_reactor_thread(true)
        .build()
        .await
        .expect("build dedicated-reactor peer connection");

    // The reactor thread should be running now (build() awaits driver init).
    #[cfg(target_os = "linux")]
    {
        let started = wait_until(Duration::from_secs(5), || {
            reactor_thread_count() == before + 1
        })
        .await;
        assert!(started, "dedicated reactor thread did not start");
    }

    // Drop WITHOUT close() — the leak scenario under test.
    drop(pc);

    // The reactor thread must terminate on drop; otherwise it (and its runtime
    // and sockets) would leak for the lifetime of the process.
    #[cfg(target_os = "linux")]
    {
        let stopped = wait_until(Duration::from_secs(5), || reactor_thread_count() == before).await;
        assert!(stopped, "dedicated reactor thread leaked after drop");
    }
}

/// Number of live OS threads named `webrtc-reactor` in this process.
///
/// The name is set via `std::thread::Builder::name`; Linux truncates thread
/// names (`comm`) to 15 bytes, so `webrtc-reactor` (14 bytes) survives intact.
#[cfg(target_os = "linux")]
fn reactor_thread_count() -> usize {
    std::fs::read_dir("/proc/self/task")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            std::fs::read_to_string(entry.path().join("comm"))
                .map(|comm| comm.trim() == "webrtc-reactor")
                .unwrap_or(false)
        })
        .count()
}

/// Poll `cond` until it is true or `timeout` elapses, yielding between checks.
#[cfg(target_os = "linux")]
async fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let step = Duration::from_millis(10);
    let mut waited = Duration::ZERO;
    loop {
        if cond() {
            return true;
        }
        if waited >= timeout {
            return false;
        }
        sleep(step).await;
        waited += step;
    }
}
