<h1 align="center">
 <a href="https://webrtc.rs"><img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/webrtc.rs.png" alt="WebRTC.rs"></a>
 <br>
</h1>
<p align="center">
 <a href="https://github.com/webrtc-rs/webrtc/actions">
  <img src="https://github.com/webrtc-rs/webrtc/workflows/webrtc/badge.svg?branch=master">
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
 A pure Rust implementation of WebRTC stack. Rewrite <a href="http://Pion.ly">Pion</a> WebRTC stack in Rust
</p>

<p align="center">
<strong>Sponsored with 💖 by</strong><br>
</p>
<p align="center">
<strong>Gold Sponsors:</strong><br>
<a href="https://www.parity.io/" target="_blank">
<img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/parity.png" style="height:75px;" alt="Parity Technologies">
</a><br>
<p align="center">
<strong>Silver Sponsors:</strong><br>
<a href="https://getstream.io/?utm_source=https://github.com/webrtc-rs/webrtc&utm_medium=github&utm_content=developer&utm_term=webrtc" target="_blank">
<img src="https://stream-blog-v2.imgix.net/blog/wp-content/uploads/f7401112f41742c4e173c30d4f318cb8/stream_logo_white.png?h=50" alt="Stream Chat">
</a><br>
<strong>Bronze Sponsors:</strong><br>
<a href="https://www.embark-studios.com/" target="_blank"><img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/embark.jpg" alt="embark"></a><br>
<a href="https://github.com/AdrianEddy" target="_blank">AdrianEddy</a><br>
<a href="https://avstack.io/?utm_medium=social&utm_source=github&utm_campaign=webrtc-rs-sponsorship" target="_blank"><img src="https://avstack.io/assets/logo-full-lightbg.b880ab9e.svg" alt="AVStack" style="width:12rem"></a>
</p>

<details>
<summary><b>Table of Content</b></summary>

- [Overview](#overview)
- [Features](#features)
- [Building](#building)
  - [Toolchain](#toolchain)
  - [Monorepo Setup](#monorepo-setup)
- [Open Source License](#open-source-license)
- [Contributing](#contributing)

</details>

## Overview

WebRTC.rs is a pure Rust implementation of WebRTC stack, which rewrites <a href="https://github.com/pion/webrtc/releases/tag/v3.1.5">Pion</a> stack in Rust.
This project is still in active and early development stage, please refer to the [Roadmap](https://github.com/webrtc-rs/webrtc/issues/1) to track the major milestones and releases.
[Examples](https://github.com/webrtc-rs/webrtc/blob/master/examples/examples/README.md) provide code samples to show how to use webrtc-rs to build media and data channel applications.

## Features

<p align="center">
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">WebRTC<a href="https://crates.io/crates/webrtc"><img src="https://img.shields.io/crates/v/webrtc.svg"></a>
    <br>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">Media<a href="https://crates.io/crates/webrtc-media"><img src="https://img.shields.io/crates/v/webrtc-media.svg"></a>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">Interceptor<a href="https://crates.io/crates/interceptor"><img src="https://img.shields.io/crates/v/interceptor.svg"></a>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">Data<a href="https://crates.io/crates/webrtc-data"><img src="https://img.shields.io/crates/v/webrtc-data.svg"></a>
    <br>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">RTP<a href="https://crates.io/crates/rtp"><img src="https://img.shields.io/crates/v/rtp.svg"></a>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">RTCP<a href="https://crates.io/crates/rtcp"><img src="https://img.shields.io/crates/v/rtcp.svg"></a>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">SRTP<a href="https://crates.io/crates/webrtc-srtp"><img src="https://img.shields.io/crates/v/webrtc-srtp.svg"></a>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">SCTP<a href="https://crates.io/crates/webrtc-sctp"><img src="https://img.shields.io/crates/v/webrtc-sctp.svg"></a>
    <br>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">DTLS<a href="https://crates.io/crates/webrtc-dtls"><img src="https://img.shields.io/crates/v/webrtc-dtls.svg"></a>
    <br>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">mDNS<a href="https://crates.io/crates/webrtc-mdns"><img src="https://img.shields.io/crates/v/webrtc-mdns.svg"></a>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">STUN<a href="https://crates.io/crates/stun"><img src="https://img.shields.io/crates/v/stun.svg"></a>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">TURN<a href="https://crates.io/crates/turn"><img src="https://img.shields.io/crates/v/turn.svg"></a>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">ICE<a href="https://crates.io/crates/webrtc-ice"><img src="https://img.shields.io/crates/v/webrtc-ice.svg"></a>
    <br>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">SDP<a href="https://crates.io/crates/sdp"><img src="https://img.shields.io/crates/v/sdp.svg"></a>
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">Util<a href="https://crates.io/crates/webrtc-util"><img src="https://img.shields.io/crates/v/webrtc-util.svg"></a>
</p>
<p align="center">
 <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/webrtc_crates_dep_graph.png" alt="WebRTC Crates Dependency Graph">
</p>
<p align="center">
 <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/webrtc_stack.png" alt="WebRTC Stack">
</p>

## Building

### Toolchain

**Minimum Supported Rust Version:** `1.60.0`

Our minimum supported rust version(MSRV) policy is to support versions of the compiler released within the last six months. We don't eagerly bump the minimum version we support, instead the minimum will be bumped on a needed by needed basis, usually because downstream dependencies force us to.

**Note:** Changes to the minimum supported version are not consider breaking from a [semver](https://semver.org/) perspective.

### Monorepo Setup

All webrtc dependent crates and examples are included in this repository at the top level in a Cargo workspace.

To build all webrtc examples:

```shell
cd examples
cargo test # build all examples (maybe very slow)
#[ or just build single example (much faster)
cargo build --example play-from-disk-vpx # build play-from-disk-vpx example only
cargo build --example play-from-disk-h264 # build play-from-disk-h264 example only
#...
#]
```

To build webrtc crate:

```shell
cargo build [or clippy or test or fmt]
```

## Open Source License

Dual licensing under both MIT and Apache-2.0 is the currently accepted standard by the Rust language community and has been used for both the compiler and many public libraries since (see <https://doc.rust-lang.org/1.6.0/complement-project-faq.html#why-dual-mitasl2-license>). In order to match the community standards, webrtc-rs is using the dual MIT+Apache-2.0 license.

## Contributing

Contributors or Pull Requests are Welcome!!!
