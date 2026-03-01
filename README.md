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
stack. The project is currently evolving into a clean, ergonomic, runtime-agnostic implementation that works with any
async runtime (Tokio, async-std, smol, embassy).

**Architecture:**

- **[rtc](https://github.com/webrtc-rs/rtc)**: Sans-I/O protocol core with complete WebRTC stack (95%+ W3C API
  compliance)
- **webrtc** (this crate): Async-friendly API with runtime abstraction layer

**📖 Learn more:** Read
our [architecture blog post](https://webrtc.rs/blog/2026/01/31/async-friendly-webrtc-architecture.html) for design
details and roadmap.

### 🚨 Important Notice: v0.17.x Feature Freeze & v0.20.0+ Development

**v0.17.x is the final feature release of the Tokio-coupled async WebRTC implementation.**

#### Current Status (February 2026)

- **v0.17.x branch**: Receives **bug fixes only** (no new features). Use this for Tokio-based production applications.
- **Master branch**: Under active development for **v0.20.0** with new Sans-I/O architecture and runtime abstraction.

#### **What's Changing in upcoming v0.20.0+?**

The new architecture will address critical issues in v0.17.x:

- ❌ Callback hell and Arc explosion
- ❌ Resources leak in callback
- ❌ Tight Tokio coupling (cannot use async-std, smol, embassy)

**v0.20.0+ will provide:**

✅ **Runtime Independence**

- Support for Tokio, async-std, smol, embassy via Quinn-style runtime abstraction
- Feature flags: `runtime-tokio` (default), `runtime-async-std`, `runtime-smol`, `runtime-embassy`

✅ **Clean Event Handling**

- Trait-based event handlers with native `async fn in trait`
- No more callback Arc cloning or `Box::new(move |...| Box::pin(async move { ... }))`
- Centralized state management with `&mut self`

✅ **Sans-I/O Foundation**

- Protocol logic completely separate from I/O (via [rtc](https://github.com/webrtc-rs/rtc) crate)
- Deterministic testing without real network I/O
- Zero-cost abstractions

#### **How to Provide Feedback**

We're actively designing v0.20.0+ and welcome your input:

- Review the [architecture blog post](https://webrtc.rs/blog/2026/01/31/async-friendly-webrtc-architecture.html)
- Join discussions on [GitHub Issues](https://github.com/webrtc-rs/webrtc/issues)
- Chat with us on [Discord](https://discord.gg/4Ju8UHdXMs)

**For production use:** Stick with v0.17.x branch until v0.20.0+ is stable.  
**For early adopters:** Follow master branch development and provide feedback!

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
