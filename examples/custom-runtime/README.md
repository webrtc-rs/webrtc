# custom-runtime

custom-runtime demonstrates that WebRTC.rs is **genuinely runtime-agnostic**: it implements the `webrtc::runtime::Runtime` trait over [`async-executor`](https://crates.io/crates/async-executor) and [`async-io`](https://crates.io/crates/async-io) — neither Tokio nor smol — and drives every runtime capability the WebRTC stack needs through it.

It doubles as the acceptance test for the runtime abstraction. Building it with **neither** `runtime-tokio` nor `runtime-smol` proves nothing in the library is hard-wired to a built-in runtime.

## Instructions

### Build custom-runtime

```shell
cargo build --example custom-runtime
```

### Run custom-runtime

The interesting invocation disables both built-in runtimes, so the only `Runtime` in the process is the one the example defines:

```shell
cargo run --no-default-features --example custom-runtime
```

Expected output:

```text
running on a custom runtime: my-runtime
sleep(50ms) took 50.304959ms
tick 0
tick 1
tick 2
timeout on a pending future => Err(Elapsed)
hello from a spawned task
udp: received "ping" from 127.0.0.1:54321
resolved localhost:3478 -> [[::1]:3478, 127.0.0.1:3478]
```

Socket binding may be denied in restricted environments (sandboxes, some CI); the UDP step then reports `udp: skipped` rather than failing.

## What it implements

Eight required `Runtime` methods, plus the three socket traits:

| Method                                        | Backed by                                    |
|-----------------------------------------------|----------------------------------------------|
| `spawn`                                       | `async_executor::Executor` on a 2-thread pool |
| `sleep`, `interval`                           | `async_io::Timer`                            |
| `wrap_udp_socket`, `wrap_tcp_listener`, `connect_tcp` | `async_io::Async<T>`                 |
| `resolve_host`                                | `std`'s resolver, offloaded to a thread      |
| `block_on`                                    | `futures_lite::future::block_on`             |

`spawn_reactor`, `yield_now` and `name` have working defaults, so they are optional. `timeout` needs no implementation at all — it is derived generically from `sleep`.

The socket wrapper sends and receives one datagram per syscall: it ignores `Transmit::segment_size` and leaves `max_gso_segments` / `max_gro_segments` at their default of 1, so the driver never asks it to batch. A production runtime would enable UDP GSO/GRO the way the built-in ones do — construct a `webrtc::runtime::UdpSocketState` at wrap time, report its `max_gso_segments()` / `gro_segments()`, and pass the whole `Transmit` to its `send`. Its `recv` already takes the same `bufs` / `meta` slices as `poll_recv` and returns the same message count, so the receive path forwards them rather than adapting anything; what it does need is to translate a count of `0` into the host runtime's not-ready protocol, and to ensure every filled message reports a `stride` of at least 1. See `src/runtime/tokio.rs` or `smol.rs`.

Reporting `max_gro_segments() > 1` is the one place a wrapper can quietly hurt itself. It tells the driver how large a receive buffer to allocate, and a coalesced segment is bounded by the **path MTU**, not by the largest datagram your application sends — size against the latter and the kernel drops the tail of each coalesced read, which shows up as unexplained packet loss rather than an error. The driver handles this for buffers it allocates; a runtime that reads into buffers of its own, such as one demultiplexing a shared socket across several connections, owns the same bound in its own loop. `AsyncUdpSocket::max_gro_segments` documents the rule.

## Using it for a real connection

The runtime is injected **per connection**, so a single process can run some connections on a custom runtime and others on a built-in one:

```rust
let runtime: Arc<dyn Runtime> = Arc::new(MyRuntime::new());

let pc = PeerConnectionBuilder::new()
    .with_runtime(runtime.clone())
    .with_handler(Arc::new(MyHandler))
    .with_udp_addrs(vec!["0.0.0.0:0"])
    .build()
    .await?;
```

There is no global registry to install into and nothing to override — which is what keeps per-connection runtime choice possible.

## Writing your own

1. Implement `Runtime` (the 8 required methods above).
2. Implement `AsyncUdpSocket`, `AsyncTcpListener` and `AsyncTcpStream` for your socket types. `AsyncUdpSocket` is poll-based: supply `poll_send` and `poll_recv`, and the four `async` convenience methods come from defaults.
3. Return your task handle from `spawn` via `JoinHandle::new(Box::new(..))`, implementing `JoinHandleInner` for it.
4. Pass it to `with_runtime`.

No `#[cfg]` edits, no fork, and no changes to library internals.

One thing to get right, and the reason this example is worth reading: **`poll_send`/`poll_recv` must be ordered to match your reactor's readiness semantics.** Event- or ticket-based reactors (async-io, and this example) must attempt the syscall *first* and consult readiness only on `WouldBlock` — otherwise datagrams already queued in the socket buffer generate no fresh readiness event and a burst stalls. Reactors with cached, level-triggered readiness (Tokio) instead need the operation to report `WouldBlock` back through their own wrapper (`try_io`) so the cache is invalidated.
