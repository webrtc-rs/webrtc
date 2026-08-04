# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Crypto provider selection** ([webrtc#839](https://github.com/webrtc-rs/webrtc/issues/839),
  [rtc#128](https://github.com/webrtc-rs/rtc/issues/128)). New `crypto-ring` (default) and `crypto-aws-lc-rs`
  Cargo features forward to `rtc`. They are additive: enabling both compiles both providers and
  `ring` remains the resolved default, so a dependency enabling `crypto-aws-lc-rs` cannot silently change
  what an application runs. Building with neither compiles no provider, and the application
  supplies its own.
- `webrtc::peer_connection::crypto` re-exports the provider API. Without it
  `SettingEngine::set_crypto_provider` was uncallable from this crate — its signature names
  `Arc<dyn RTCCryptoProvider>`, which a user had no way to spell.
- Provider selection is per peer connection, so two connections in one process can use different
  providers. `tests/crypto_provider_integration.rs` covers same-provider, cross-provider, and
  application-supplied pairings end to end — ICE/STUN integrity, the DTLS handshake, and SRTP
  media over a live connection.
- Document pre-1.0 API stability / extensibility policy (see `docs/semver.md`).
### Changed

- No cryptography happens in this crate. The TURN client now takes its provider from the peer
  connection (`RTCPeerConnection::crypto_provider()`) rather than constructing one, so a
  connection uses exactly one provider throughout. A CI check asserts `webrtc` depends on no
  crypto implementation.

-

### Deprecated

-

### Removed

-

### Fixed

-

### Security

-

## [0.20.0] - 2026-07-31

### Added

- The async `webrtc` v0.20.0 is a clean, ergonomic, runtime-agnostic rewrite on top of a Sans-I/O core `rtc`;
- It ships with Tokio and smol runtime backends, and any other runtime can be plugged in by implementing one trait.

[Unreleased]: https://github.com/webrtc-rs/webrtc/compare/0.20.0...HEAD

[0.20.1]: https://github.com/webrtc-rs/webrtc/compare/0.20.0...0.20.1

[0.20.0]: https://github.com/webrtc-rs/webrtc/releases/tag/0.20.0
