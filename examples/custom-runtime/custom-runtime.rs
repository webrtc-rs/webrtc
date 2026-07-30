//! A third-party async runtime plugged into `webrtc`.
//!
//! This example implements [`webrtc::runtime::Runtime`] over `async-executor` +
//! `async-io` — neither tokio nor smol — to demonstrate that the runtime abstraction is
//! genuinely pluggable. It is the acceptance test for that design:
//!
//! ```text
//! cargo run --no-default-features --example custom-runtime
//! ```
//!
//! Building with **neither** `runtime-tokio` nor `runtime-smol` proves nothing in the
//! library is hard-wired to a built-in runtime. All that is required is:
//!
//! * one `impl Runtime` (8 methods), plus
//! * the three socket traits, then
//! * `PeerConnectionBuilder::with_runtime(Arc::new(MyRuntime::new()))`
//!
//! No `#[cfg]` edits, no fork, and no changes to library internals. Because the runtime is
//! injected per connection, a process can run some connections on this runtime and others
//! on a different one.

mod my_runtime;

use std::sync::Arc;
use std::time::Duration;
use webrtc::runtime::Runtime;

// ── Demonstration ─────────────────────────────────────────────────────────────

fn main() {
    env_logger::init();

    let runtime: Arc<dyn Runtime> = Arc::new(my_runtime::MyRuntime::new());
    println!("running on a custom runtime: {}", runtime.name());

    // Every capability the WebRTC stack needs now comes from `runtime`.
    runtime.block_on(Box::pin(async {
        // Timers.
        let started = std::time::Instant::now();
        runtime.sleep(Duration::from_millis(50)).await;
        println!("sleep(50ms) took {:?}", started.elapsed());

        // Repeating timer.
        let mut ticker = runtime.interval(Duration::from_millis(20));
        for n in 0..3 {
            ticker.tick().await;
            println!("tick {n}");
        }

        // Timeout, derived generically from `Runtime::sleep`.
        let timed_out = webrtc::runtime::timeout(
            &*runtime,
            Duration::from_millis(30),
            std::future::pending::<()>(),
        )
        .await;
        println!("timeout on a pending future => {timed_out:?}");

        // Task spawning.
        let (tx, rx) = async_channel::bounded::<&str>(1);
        let handle = runtime.spawn(Box::pin(async move {
            let _ = tx.send("hello from a spawned task").await;
        }));
        println!("{}", rx.recv().await.expect("task sent a value"));
        drop(handle);

        // UDP round-trip through the custom socket wrapper. Binding can be denied by a
        // sandbox, so treat failure as "skipped" rather than aborting the demo.
        match (
            std::net::UdpSocket::bind("127.0.0.1:0"),
            std::net::UdpSocket::bind("127.0.0.1:0"),
        ) {
            (Ok(sa), Ok(sb)) => {
                let a = runtime.wrap_udp_socket(sa).expect("wrap a");
                let b = runtime.wrap_udp_socket(sb).expect("wrap b");
                let b_addr = b.local_addr().expect("b addr");

                a.send_to(b"ping", b_addr).await.expect("send");
                let mut buf = [0u8; 16];
                let (n, from) = b.recv_from(&mut buf).await.expect("recv");
                println!(
                    "udp: received {:?} from {from}",
                    std::str::from_utf8(&buf[..n]).unwrap_or("<invalid utf8>")
                );
            }
            _ => println!("udp: skipped (socket binding not permitted in this environment)"),
        }

        // DNS.
        match runtime.resolve_host("localhost:3478").await {
            Ok(addrs) => println!("resolved localhost:3478 -> {addrs:?}"),
            Err(err) => println!("resolve failed (may be sandboxed): {err}"),
        }
    }));

    println!(
        "\nAll runtime capabilities exercised without tokio or smol.\n\
         To use it for a connection:\n\
         \n    PeerConnectionBuilder::new()\n\
         \x20       .with_runtime(runtime.clone())\n\
         \x20       .with_udp_addrs(vec![\"0.0.0.0:0\"])\n\
         \x20       .build()\n\
         \x20       .await?;\n"
    );
}
