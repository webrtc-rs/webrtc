//! Regression tests for the bounded shared reactor pool (issue #101).
//!
//! `with_dedicated_reactor_thread(true)` no longer spawns one OS thread per
//! `PeerConnection`; it pins each driver to a thread drawn from a process-global
//! pool of at most `N` threads (`set_reactor_pool_size`). This test asserts the
//! two properties that replace the old one-thread-per-connection model:
//!
//! 1. **Bounded threads (the RSS win):** building `M > N` dedicated-reactor
//!    connections creates at most `N` reactor threads, not `M`. On Linux we prove
//!    it directly by counting OS threads named `webrtc-rx*` in `/proc/self/task`.
//!    Each file under `tests/` is its own test binary (own process), so this
//!    thread count — and the process-global pool size — is not polluted by other
//!    tests.
//!
//! 2. **Drop terminates the driver task (no leak):** dropping a connection
//!    *without* `close()` must stop its driver task so the connection's state and
//!    buffers are freed (the pool thread itself is shared and persists). We prove
//!    the driver task released the connection by watching a `Weak` to the event
//!    handler — which only the driver task transitively holds once the app drops
//!    the connection — become dead.
use std::sync::Arc;
use std::time::Duration;

use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{block_on, set_reactor_pool_size, sleep};

/// Small pool so `N_CONNECTIONS > POOL_SIZE` and the bound actually bites.
const POOL_SIZE: usize = 2;
const N_CONNECTIONS: usize = 5;

struct NoopHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for NoopHandler {}

#[test]
fn test_reactor_pool_is_bounded_and_drop_stops_driver_task() {
    // Size the process-global pool before the first dedicated-reactor build.
    set_reactor_pool_size(POOL_SIZE);
    block_on(run());
}

async fn run() {
    #[cfg(target_os = "linux")]
    let before = reactor_thread_count();

    // Build M > N dedicated-reactor connections. Keep only a `Weak` to each
    // handler: after the connection is dropped, the sole remaining strong ref is
    // the one the driver task holds (via its `Arc<PeerConnectionRef>`), so the
    // Weak dies exactly when that task terminates and releases the connection.
    let mut pcs = Vec::new();
    let mut handler_weaks = Vec::new();
    for _ in 0..N_CONNECTIONS {
        let handler = Arc::new(NoopHandler);
        handler_weaks.push(Arc::downgrade(&handler));
        let pc = PeerConnectionBuilder::new()
            .with_handler(handler) // moves the only app-side strong ref into the builder
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .with_dedicated_reactor_thread(true)
            .build()
            .await
            .expect("build dedicated-reactor peer connection");
        pcs.push(pc);
    }

    // Property 1: at most POOL_SIZE reactor threads exist, regardless of the
    // N_CONNECTIONS drivers sharing them.
    #[cfg(target_os = "linux")]
    {
        let bounded = wait_until(Duration::from_secs(5), || {
            reactor_thread_count() == before + POOL_SIZE
        })
        .await;
        assert!(
            bounded,
            "expected exactly {POOL_SIZE} reactor threads for {N_CONNECTIONS} connections, \
             found {} (pool not bounded / not shared)",
            reactor_thread_count() - before
        );
    }

    // Drop every connection WITHOUT close() — the leak scenario under test.
    drop(pcs);

    // Property 2: each driver task terminates on drop and releases its connection
    // state (its handler Weak dies). Without the `Drop` shutdown signal the task
    // would keep running on its pool thread and this would spin out the timeout.
    let stopped = wait_until(Duration::from_secs(5), || {
        handler_weaks.iter().all(|w| w.strong_count() == 0)
    })
    .await;
    assert!(
        stopped,
        "a dropped connection's driver task did not terminate (leaked task/state)"
    );

    // Property 1 (still): the pool threads are shared and persist after the
    // connections are gone — they are NOT torn down per connection.
    #[cfg(target_os = "linux")]
    {
        assert_eq!(
            reactor_thread_count(),
            before + POOL_SIZE,
            "reactor pool threads should persist after connections are dropped"
        );
    }

    // Liveness: the pool is still usable after those drops — a fresh
    // dedicated-reactor connection builds on it and reuses the same bounded pool.
    // Uses the `with_reactor_pool_size` builder knob (equivalent to the free
    // `set_reactor_pool_size` used above; the pool is already sized, so this is a
    // no-op here) to exercise that path.
    let pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(NoopHandler))
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_dedicated_reactor_thread(true)
        .with_reactor_pool_size(POOL_SIZE)
        .build()
        .await
        .expect("reactor pool unusable after drops");
    #[cfg(target_os = "linux")]
    assert_eq!(
        reactor_thread_count(),
        before + POOL_SIZE,
        "a new connection must reuse the existing pool, not grow it"
    );
    pc.close().await.expect("close");
}

/// Number of live OS threads whose name starts with `webrtc-rx` in this process.
///
/// The name is set via `std::thread::Builder::name`; Linux truncates thread
/// names (`comm`) to 15 bytes, so `webrtc-rx{i}` survives intact for realistic
/// pool sizes.
#[cfg(target_os = "linux")]
fn reactor_thread_count() -> usize {
    std::fs::read_dir("/proc/self/task")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            std::fs::read_to_string(entry.path().join("comm"))
                .map(|comm| comm.trim().starts_with("webrtc-rx"))
                .unwrap_or(false)
        })
        .count()
}

/// Poll `cond` until it is true or `timeout` elapses, yielding between checks.
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
