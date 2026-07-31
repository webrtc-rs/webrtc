<h1 align="center">
 <a href="https://webrtc.rs"><img src="https://raw.githubusercontent.com/webrtc-rs/webrtc-rs.github.io/master/res/webrtc.rs.png" alt="WebRTC.rs"></a>
 <br>
</h1>
<p align="center">
 <a href="https://github.com/webrtc-rs/webrtc/actions">
  <img src="https://github.com/webrtc-rs/webrtc/workflows/cargo/badge.svg">
 </a>
 <a href="https://codecov.io/gh/webrtc-rs/webrtc">
  <img src="https://codecov.io/gh/webrtc-rs/webrtc/branch/master/graph/badge.svg">
 </a>
 <a href="https://deps.rs/repo/github/webrtc-rs/webrtc">
  <img src="https://deps.rs/repo/github/webrtc-rs/webrtc/status.svg">
 </a>
 <a href="https://crates.io/crates/webrtc">
  <img src="https://img.shields.io/crates/v/webrtc.svg">
 </a>
 <a href="https://docs.rs/webrtc">
  <img src="https://docs.rs/webrtc/badge.svg">
 </a>
 <a href="https://doc.rust-lang.org/1.6.0/complement-project-faq.html#why-dual-mitasl2-license">
  <img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue" alt="License: MIT/Apache 2.0">
 </a>
 <a href="https://discord.gg/4Ju8UHdXMs">
  <img src="https://img.shields.io/discord/800204819540869120?logo=discord" alt="Discord">
 </a>
 <a href="https://twitter.com/WebRTCrs">
  <img src="https://img.shields.io/twitter/url/https/twitter.com/webrtcrs.svg?style=social&label=%40WebRTCrs" alt="Twitter">
 </a>
</p>
<p align="center">
 Async-friendly WebRTC implementation in Rust
</p>

<p align="center">
<strong>Sponsored with 💖 by</strong><br>
</p>
<p align="center">
<strong>Gold Sponsors:</strong><br>
<a href="https://www.recall.ai" target="_blank">
<img src="https://raw.githubusercontent.com/webrtc-rs/webrtc-rs.github.io/master/res/recall_md.svg"
alt="Recall.ai">
</a><br>
<p align="center">
<strong>Silver Sponsors:</strong><br>
<a href="https://getstream.io/video/voice-calling/?utm_source=https://github.com/webrtc-rs/webrtc&utm_medium=sponsorship&utm_content=&utm_campaign=webrtcRepo_July2023_video_klmh22" target="_blank">
<img src="https://raw.githubusercontent.com/webrtc-rs/webrtc-rs.github.io/master/res/stream-logo.png" height="50" alt="Stream Chat">
</a><br>
<a href="https://channel.io/" target="_blank">
<img src="https://raw.githubusercontent.com/webrtc-rs/webrtc-rs.github.io/master/res/ChannelTalk_logo_md.png" alt="ChannelTalk">
</a><br>
<strong>Bronze Sponsors:</strong><br>
<a href="https://github.com/AdrianEddy" target="_blank">AdrianEddy</a><br>
</p>

<!--details>
<summary><b>Table of Content</b></summary>

- [Overview](#overview)
- [Open Source License](#open-source-license)
- [Contributing](#contributing)

</details-->

## Overview

WebRTC.rs is an async-friendly WebRTC implementation in Rust, originally inspired by and largely rewriting the Pion stack. The async `webrtc` crate is a clean, ergonomic, runtime-agnostic rewrite on top of a Sans-I/O core; it ships with Tokio and smol runtime backends, and any other runtime can be plugged in by implementing one trait.

**Architecture:**

- **[rtc](https://github.com/webrtc-rs/rtc)**: Sans-I/O protocol core with complete WebRTC stack (95%+ W3C API
  compliance)
- **webrtc** (this crate): a thin async layer over `rtc`:
    - **`PeerConnection`** — the user-facing async API handle; all operations (create offers/answers, add tracks,
      create data channels) are `async`
    - **`PeerConnectionDriver`** — an internal background event loop, spawned automatically, that owns the sockets,
      drives the Sans-I/O `rtc` core, handles timeouts, and dispatches events
    - **`Runtime`** — a trait abstracting timers, task spawning, and sockets, so the crate is runtime-agnostic

**📖 Learn more:** Read
our [architecture blog post](https://webrtc.rs/blog/2026/01/31/async-friendly-webrtc-architecture.html) for design
details and roadmap.

## Getting Started

```toml
[dependencies]
webrtc = "0.20"
```

Or with the smol runtime instead of Tokio:

```toml
[dependencies]
webrtc = { version = "0.20", default-features = false, features = ["runtime-smol"] }
```

**Feature flags:**

| Feature         | Default | Description                                                             |
|-----------------|---------|-------------------------------------------------------------------------|
| `runtime-tokio` | ✅      | Timers, task spawning and sockets via Tokio                             |
| `runtime-smol`  |         | The same, via smol                                                      |
| `runtime-mock`  |         | `MockRuntime`, a deterministic virtual-clock runtime for tests (no I/O) |

The runtime features are **additive**: each one only makes a built-in runtime *available*, so enabling several is safe and a single process can drive different connections on different runtimes.

**Bringing your own runtime.** The built-ins are not privileged — implement `webrtc::runtime::Runtime` and pass it per connection with `with_runtime`, with no `#[cfg]` edits and no fork. See the [custom-runtime example](examples/custom-runtime), which runs the full stack on `async-executor` + `async-io` with `--no-default-features` (neither Tokio nor smol compiled in).

Build a peer connection and create an offer:

```rust
use std::sync::Arc;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCIceServer, RTCPeerConnectionIceEvent,
};
use webrtc::runtime::TokioRuntime;

// 1. Implement the PeerConnectionEventHandler trait to handle events
#[derive(Clone)]
struct MyHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for MyHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        println!("New local ICE candidate gathered: {}", event.candidate);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 2. Configure the peer connection
    let config = RTCConfigurationBuilder::default()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }])
        .build();

    // 3. Build the PeerConnection — the background driver starts here.
    //    The runtime is a value, injected per connection: swap `TokioRuntime` for
    //    `SmolRuntime`, or for your own `Runtime` impl, and nothing else changes.
    //    (Omit `with_runtime` entirely and `build()` uses the compiled-in default.)
    let pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_runtime(Arc::new(TokioRuntime))
        .with_handler(Arc::new(MyHandler))
        .with_udp_addrs(vec!["0.0.0.0:0"])
        .build()
        .await?;

    // 4. Create an SDP offer and set it as the local description
    let offer = pc.create_offer(None).await?;
    pc.set_local_description(offer).await?;

    Ok(())
}
```

`build()` returns an opaque `impl PeerConnection`. `PeerConnection` is an object-safe trait, so when you need to store
the connection in a struct or share it across tasks, wrap it:

```rust,ignore
let pc: Arc<dyn PeerConnection> = Arc::new(pc);
```

Either way no runtime or interceptor type parameters leak into your own types.

**Next steps:** browse the [API docs](https://docs.rs/webrtc) or the
[35 runnable examples](https://github.com/webrtc-rs/webrtc/tree/master/examples) — data channels, media playback,
simulcast, ICE restart, insertable streams, and more.

### 🚨 v0.17.x → v0.20.0: the Sans-I/O rewrite

**`v0.20.0` is the first stable release of the new Sans-I/O, runtime-agnostic architecture.** It supersedes the
Tokio-coupled `v0.17.x` line, which is now in bug-fix-only maintenance.

#### Current Status

- **`v0.20.x`** (master): The current line, and the recommended choice for all new projects. Runtime-agnostic,
  Sans-I/O, with the `PeerConnection` handle + background driver design described above.
- **`v0.17.x`**: Receives **bug fixes only** (no new features). Still a valid choice if you have an existing
  Tokio-coupled integration you are not ready to migrate.

Note that `v0.20.0` is a breaking change from `v0.17.x` — the event-handler traits replace callbacks, and the API is
async throughout. While the version is `0.x`, a minor bump may carry breaking changes
(see [Semantic Versioning](#semantic-versioning)).

#### What v0.20.0 delivers

The rewrite resolves the core pain points of `v0.17.x` — callback hell and `Arc` explosion, resource leaks in
callbacks, and tight Tokio coupling:

✅ **Runtime independence**

- Runtime-agnostic via a Quinn-style `Runtime` abstraction (timers, task spawning, sockets, DNS)
- Feature flags: **`runtime-tokio`** (default) and **`runtime-smol`**, additive rather than mutually exclusive
- Any third-party runtime works today: implement `Runtime`, inject it per connection with `with_runtime`. The [custom-runtime example](examples/custom-runtime) does exactly that on `async-executor` + `async-io`, with neither built-in runtime compiled in
- **`runtime-mock`** gives tests a deterministic virtual clock, so timing-dependent behaviour is testable instantly and without sockets

✅ **Clean event handling**

- One trait-based event handler (`PeerConnectionEventHandler`) replaces per-event callback registration, with
  default no-op methods so you implement only what you need
- No more callback `Arc` cloning or `Box::new(move |...| Box::pin(async move { ... }))`
- Centralized state: the handler is shared as a single `Arc<MyHandler>` instead of an `Arc::clone` per callback.
  Methods take `&self`, so mutable handler state goes behind one lock rather than being captured per closure

✅ **Sans-I/O foundation**

- Protocol logic completely separate from I/O (via the [rtc](https://github.com/webrtc-rs/rtc) core)
- Deterministic testing without real network I/O
- A thin async driver (`PeerConnection` handle + background `PeerConnectionDriver`) over the core

#### How to Provide Feedback

We welcome your input as `v0.20.x` grows:

- Review the [architecture blog post](https://webrtc.rs/blog/2026/01/31/async-friendly-webrtc-architecture.html)
- Join discussions on [GitHub Issues](https://github.com/webrtc-rs/webrtc/issues)
- Chat with us on [Discord](https://discord.gg/4Ju8UHdXMs)

**New projects:** start on `v0.20`.  
**Migrating from `v0.17.x`?** Open an issue if you hit a gap — migration reports directly shape what we prioritise.

## Building and Testing

```bash
# Update rtc submodule first
git submodule update --init --recursive

# Build the library
cargo build

# Run tests
cargo test

# Build documentation
cargo doc --open

# Run examples
cargo run --example data-channels
```

## Semantic Versioning

This project follows [Semantic Versioning](https://semver.org/):

- **Patch** (`0.x.Y`): Bug fixes and internal improvements with no public API changes.
- **Minor** (`0.X.0`): Backwards-compatible additions or deprecations to the public API.
- **Major** (`X.0.0`): Breaking changes to the public API.

While the version is `0.x`, the minor version acts as the major — i.e., a minor bump may include breaking changes. Once
`1.0.0` is released, full semver stability guarantees apply.

Pre-release versions are published with the following suffixes, in order of increasing stability:

- **`-alpha.N`**: Early preview. API is unstable and may change significantly.
- **`-beta.N`**: Feature-complete for the release. API may still have minor changes.
- **`-rc.N`**: Release candidate. No further API changes are expected unless critical issues are found.

For example: `1.0.0-alpha.1` → `1.0.0-beta.1` → `1.0.0-rc.1` → `1.0.0`.

## Open Source License

Dual licensing under both MIT and Apache-2.0 is the currently accepted standard by the Rust language community and has
been used for both the compiler and many public libraries since (
see <https://doc.rust-lang.org/1.6.0/complement-project-faq.html#why-dual-mitasl2-license>). In order to match the
community standards, webrtc-rs is using the dual MIT+Apache-2.0 license.

## Contributing

Contributors or Pull Requests are Welcome!!!
