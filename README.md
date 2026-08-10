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

WebRTC.rs is an async-friendly WebRTC implementation in Rust, originally inspired by and largely rewriting the Pion
stack. The async `webrtc` crate is a clean, ergonomic, runtime-agnostic rewrite on top of a Sans-I/O core; it ships with
Tokio and smol runtime backends, and any other runtime can be plugged in by implementing one trait.

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

**Trying the `0.21` pre-release?** Cargo does not select pre-release versions from a plain
`"0.21"` requirement, so name it in full:

```toml
[dependencies]
webrtc = "0.21.0-alpha.1"
```

**Feature flags:**

| Feature            | Default | Description                                                             |
|--------------------|---------|-------------------------------------------------------------------------|
| `runtime-tokio`    | ✅       | Timers, task spawning and sockets via Tokio                             |
| `runtime-smol`     |         | The same, via smol                                                      |
| `runtime-mock`     |         | `MockRuntime`, a deterministic virtual-clock runtime for tests (no I/O) |
| `crypto-ring`      | ✅       | The `ring`-based crypto provider                                        |
| `crypto-aws-lc-rs` |         | The `aws-lc-rs`-based crypto provider                                   |

The runtime features are **additive**: each one only makes a built-in runtime *available*, so enabling several is safe
and a single process can drive different connections on different runtimes.

The crypto features work the same way. Enabling both compiles both providers, and `ring` stays the default selection —
so a dependency that turns on `crypto-aws-lc-rs` cannot silently change which one your application runs. Building with
neither compiles no provider, and you supply your own.

**Bringing your own runtime.** The built-ins are not privileged — implement `webrtc::runtime::Runtime` and pass it per
connection with `with_runtime`, with no `#[cfg]` edits and no fork. See
the [custom-runtime example](examples/custom-runtime), which runs the full stack on `async-executor` + `async-io` with
`--no-default-features` (neither Tokio nor smol compiled in).

**Choosing a crypto provider.** Same story: pass one per connection through `SettingEngine`, which also means two
connections in one process can use different providers.

```rust
use std::sync::Arc;
use webrtc::peer_connection::crypto;
use webrtc::peer_connection::SettingEngineBuilder;

let setting_engine = SettingEngineBuilder::new().with_crypto_provider(Arc::new(crypto::providers::AwsLcRsProvider::new()));
```

Applications needing a FIPS-validated module, an HSM, or a platform backend implement
`crypto::RTCCryptoProvider` and pass it the same way; `rtc-crypto`'s conformance suite validates
an implementation against the same RFC vectors the built-ins pass. No cryptography happens in this
crate — it forwards the provider to `rtc`.

Build a peer connection and create an offer:

```rust
use std::sync::Arc;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCIceServer, RTCPeerConnectionIceEvent, SettingEngine, crypto,
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

    // 3. Choose the crypto provider (optional).
    //    `crypto-ring` is on by default, so this states what you would get anyway — but the
    //    choice is per connection, not per process, so two connections here could use
    //    different providers. Enable `crypto-aws-lc-rs` for `AwsLcRsProvider`, or build with
    //    neither feature and pass your own `RTCCryptoProvider`.
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_crypto_provider(Arc::new(crypto::providers::RingProvider::new()));

    // 4. Build the PeerConnection — the background driver starts here.
    //    The runtime is a value, injected per connection: swap `TokioRuntime` for
    //    `SmolRuntime`, or for your own `Runtime` impl, and nothing else changes.
    //    (Omit `with_runtime` entirely and `build()` uses the compiled-in default.)
    let pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_setting_engine(setting_engine)
        .with_runtime(Arc::new(TokioRuntime))
        .with_handler(Arc::new(MyHandler))
        .with_udp_addrs(vec!["0.0.0.0:0"])
        .build()
        .await?;

    // 5. Create an SDP offer and set it as the local description
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
[37 runnable examples](https://github.com/webrtc-rs/webrtc/tree/master/examples) — data channels, media playback,
simulcast, ICE restart, insertable streams, and more.

### The architecture

✅ **Runtime independence**

- Runtime-agnostic via a Quinn-style `Runtime` abstraction (timers, task spawning, sockets, DNS)
- Feature flags: **`runtime-tokio`** (default) and **`runtime-smol`**, additive rather than mutually exclusive
- Any third-party runtime works today: implement `Runtime`, inject it per connection with `with_runtime`.
  The [custom-runtime example](examples/custom-runtime) does exactly that on `async-executor` + `async-io`, with neither
  built-in runtime compiled in
- **`runtime-mock`** gives tests a deterministic virtual clock, so timing-dependent behaviour is testable instantly and
  without sockets

✅ **Clean event handling**

- One trait-based event handler (`PeerConnectionEventHandler`) for every event, with default no-op methods so you
  implement only what you need — no per-event callback registration
- Centralized state: the handler is one shared `Arc<MyHandler>`. Methods take `&self`, so mutable handler state
  goes behind a single lock rather than being captured per closure

✅ **Sans-I/O foundation**

- Protocol logic completely separate from I/O (via the [rtc](https://github.com/webrtc-rs/rtc) core)
- Deterministic testing without real network I/O
- A thin async driver (`PeerConnection` handle + background `PeerConnectionDriver`) over the core

### Release lines

- **`v0.21.x`** — in development, currently `v0.21.0-alpha.1`. The pre-1.0 line, working towards a stable public
  API; see [what's in 0.21](#whats-in-021) below. APIs may change between alphas.
- **`v0.20.x`** — the current stable line, and the recommended choice for production today.

While the version is `0.x`, a minor bump may carry breaking changes
(see [Semantic Versioning](#semantic-versioning)). Once `1.0` ships, that stops being true — which is the whole
point of the `0.21` work.

### What's in 0.21

`v0.21` is the run-up to `1.0`, whose goal is a **stable public API** — not every feature, but an API that will
not break under you. Work so far:

✅ **Extensibility, before the freeze** — public enums are `#[non_exhaustive]` and the traits this crate alone
implements are sealed, so variants and methods can be added later without a major bump. This cannot be done
*after* `1.0`, which is why it came first.

✅ **Crypto provider selection** — `crypto-ring` (default) and `crypto-aws-lc-rs` features, chosen per peer
connection, so two connections in one process can use different providers. Build with neither and supply your
own. No cryptography happens in this crate; a CI check enforces that.

✅ **Deterministic time** — the Sans-I/O core no longer reads a clock. Time is an input, threaded from
`Runtime::now()`, so `runtime-mock`'s virtual clock genuinely drives ICE timeouts, DTLS retransmits and SCTP
RTO. Advancing a mock clock by 30 s now produces real protocol transitions instead of nothing.

✅ **No silent drops on data channels** — a slow consumer used to lose messages on a *reliable* channel once
the internal hand-off queue filled. The driver now keeps them and stops pulling from the core, so back-pressure
reaches SCTP's receive window and the peer is throttled instead. Media is unaffected by a stalled data channel.

Track the remaining work in [The path to webrtc 1.0](https://github.com/webrtc-rs/webrtc/issues/836).

### How to Provide Feedback

We welcome your input as `v0.20.x` and `v0.21.x` grow:

- Review the [architecture blog post](https://webrtc.rs/blog/2026/01/31/async-friendly-webrtc-architecture.html)
- Join discussions on [GitHub Issues](https://github.com/webrtc-rs/webrtc/issues)
- Chat with us on [Discord](https://discord.gg/4Ju8UHdXMs)

**New projects:** start on `v0.20`, or on `v0.21.0-alpha.1` if you want the pre-1.0 API and can absorb changes
between alphas.  
**Hit a gap?** Open an issue — reports of what is missing directly shape what we prioritise before `1.0`.

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
