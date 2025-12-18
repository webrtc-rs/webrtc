<h1 align="center">
 <a href="https://webrtc.rs"><img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/webrtc.rs.png" alt="WebRTC.rs"></a>
 <br>
</h1>
<p align="center">
 <a href="https://github.com/webrtc-rs/webrtc/actions">
  <img src="https://github.com/webrtc-rs/webrtc/workflows/cargo/badge.svg?branch=master">
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
<img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/recall.svg"
alt="Recall.ai">
</a><br>
<p align="center">
<strong>Silver Sponsors:</strong><br>
<a href="https://getstream.io/video/voice-calling/?utm_source=https://github.com/webrtc-rs/webrtc&utm_medium=sponsorship&utm_content=&utm_campaign=webrtcRepo_July2023_video_klmh22" target="_blank">
<img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/stream-logo.png" height="50" alt="Stream Chat">
</a><br>
<a href="https://channel.io/" target="_blank">
<img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/ChannelTalk_logo.png" alt="ChannelTalk">
</a><br>
<strong>Bronze Sponsors:</strong><br>
<a href="https://github.com/AdrianEddy" target="_blank">AdrianEddy</a><br>
</p>

<details>
<summary><b>Table of Content</b></summary>

- [Overview](#overview)
- [Features](#features)
- [Building](#building)
- [Open Source License](#open-source-license)
- [Contributing](#contributing)

</details>

## Overview

WebRTC.rs is an async-friendly WebRTC implementation in Rust, originally inspired by and largely rewriting the Pion
stack. The project is under active development and should be considered early stage; please refer to the
[Roadmap](https://github.com/webrtc-rs/webrtc/issues/1) for planned milestones and releases.
The [Examples](https://github.com/webrtc-rs/webrtc/blob/master/examples/examples/README.md) demonstrate how to build
media and data-channel applications using webrtc-rs.

The project is shifting its focus toward a Sans-IO WebRTC implementation in Rust.
Going forward, the [webrtc-rs/webrtc](https://github.com/webrtc-rs/webrtc) crate will be built on top of the
[webrtc-rs/rtc](https://github.com/webrtc-rs/rtc) crate, which provides a clean, protocol-centric Sans-IO core.
This architecture enables a fully async-friendly WebRTC implementation in Rust.

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
    <img src="https://raw.githubusercontent.com/webrtc-rs/webrtc/master/doc/check.png">DTLS<a href="https://crates.io/crates/dtls"><img src="https://img.shields.io/crates/v/dtls.svg"></a>
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

Dual licensing under both MIT and Apache-2.0 is the currently accepted standard by the Rust language community and has
been used for both the compiler and many public libraries since (
see <https://doc.rust-lang.org/1.6.0/complement-project-faq.html#why-dual-mitasl2-license>). In order to match the
community standards, webrtc-rs is using the dual MIT+Apache-2.0 license.

## Contributing

Contributors or Pull Requests are Welcome!!!
