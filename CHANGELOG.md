# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

-

### Changed

- **Bind addresses are resolved on every bind, and a wildcard means "every interface"**
  ([webrtc#874](https://github.com/webrtc-rs/webrtc/issues/874)). `with_udp_addrs` /
  `with_tcp_addrs` values are kept as configured instead of being resolved once at construction,
  so the ICE-restart rebind added in [webrtc#868](https://github.com/webrtc-rs/webrtc/issues/868)
  re-resolves them: a host name follows its DNS record, and `0.0.0.0` / `[::]` re-enumerates the
  local interfaces. A wildcard is no longer bound verbatim — one socket is bound per interface
  address (skipping loopback and link-local), which is what makes its host candidates usable, and
  what lets an ICE restart after a Wi-Fi/cellular handover pick up the interfaces the device has
  now. On a host with no usable interface the wildcard is bound as before. Because the configured
  addresses now outlive `build()`, `PeerConnectionBuilder::build` requires `A: Send + 'static` —
  owned addresses (`String`, `SocketAddr`, `&'static str`) are unaffected.
- **An address that cannot be bound is skipped rather than fatal**
  ([webrtc#874](https://github.com/webrtc-rs/webrtc/issues/874)). The failure is logged — `warn`
  for an enumerated interface address, `error` for one the application configured — and binding
  continues with the rest; only binding nothing at all is still an error. An address left behind
  by a network handover (`EADDRNOTAVAIL`) therefore no longer costs the connection the interfaces
  that are still there.

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
