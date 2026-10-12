# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **TCP multiplexing: many peer connections over one shared ICE-TCP listener** — the TCP
  sibling of the UDP mux: new `webrtc::peer_connection::transport::tcp_mux` module with a
  `TCPMux` trait, the built-in `TCPMuxDefault`, and `PeerConnectionBuilder::with_tcp_mux`.
  The mux accepts on one listener and dispatches each stream to its connection by the ufrag in
  the first framed STUN message (replaying the peeked frame), so a whole deployment can share
  one passive ICE-TCP port. Only the passive side is multiplexed; active dials need no mux.
  `tests/tcp_mux.rs` runs two connections on one fixed TCP port end to end (ICE-TCP-only
  peers, data-channel echo).
- **UDP multiplexing: many peer connections over one shared UDP socket** — new
  `webrtc::peer_connection::transport::udp_mux` module with a `UDPMux` trait and the built-in
  `UDPMuxDefault`. Handing one mux to every `PeerConnectionBuilder::with_udp_mux` serves any
  number of connections from a single fixed UDP port (the SRS/mediamtx deployment shape),
  where a pinned `with_udp_addrs` port fails the second connection's bind with `EADDRINUSE`.
  The mux's reader demultiplexes inbound datagrams by the local ICE ufrag in each STUN check
  and by learnt peer address for ufrag-less traffic (DTLS/SRTP/SRTCP); candidates are
  advertised per interface on the mux's port, with inbound packets attributed to an interface
  via `RecvMeta::dst_ip` (packet info) and outbound ones pinned with `Transmit::src_ip` on a
  wildcard listen. Muxed connections gather host candidates only (the restriction Pion's
  `ICEUDPMux` documents), and their ICE credentials are pinned at build time so the route
  survives ICE restarts. `tests/udp_mux.rs` runs two connections on one fixed port end to end.
  Requires `rtc` ≥ the commit that adds `SettingEngine::set_ice_credentials`/
  `ice_credentials`.
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
