# webrtc-dtls changelog

## Unreleased

## v0.7.1

* Added support for insecure/deprecated signature verification algorithms [#342](https://github.com/webrtc-rs/webrtc/pull/342) by [@chuigda](https://github.com/chuigda).

## v0.7.0

* Increased minimum support rust version to `1.60.0`.
* Add `RTCCertificate::from_pem` and `RTCCertificate::serialize_pem` (only work with `pem` feature enabled) [#333](https://github.com/webrtc-rs/webrtc/pull/333)

### Breaking

* Increased required `webrtc-util` version to `0.7.0`, with this change some methods in `DTLSConn` that implement `webrtc_util::Conn` have changed from async to sync.

## v0.6.0

* [#254 [DTLS] Add NamedCurve::P384](https://github.com/webrtc-rs/webrtc/pull/254) contributed by [neonphog](https://github.com/neonphog)
* Increased min version of `log` dependency to `0.4.16`. [#250 Fix log at ^0.4.16 to make tests compile](https://github.com/webrtc-rs/webrtc/pull/250) by [@k0nserv](https://github.com/k0nserv).
* Increased serde's minimum version to 1.0.110 [#243 Fixes for cargo minimal-versions](https://github.com/webrtc-rs/webrtc/pull/243) contributed by [algesten](https://github.com/algesten)

## Prior to 0.6.0

Before 0.6.0 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/dtls/releases).
