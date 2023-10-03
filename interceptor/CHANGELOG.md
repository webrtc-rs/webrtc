# interceptor changelog

## Unreleased

## v0.9.0

* Fix over-NACK due not resetting lost_packets bitmask [\#372](https://github.com/webrtc-rs/webrtc/pull/372/).
* Further extended stats interceptors to collect stats for `RemoteOutoundRTPStats` and improve `RemoteInboundRTPStats` collection. [#282](https://github.com/webrtc-rs/webrtc/pull/282) by [@k0nserv](https://github.com/k0nserv).
* When generating periodic TWCC feedback packets we no longer burst several packets in a row to catch up, i.e., we now use `MissedTickBehavior::Skip` instead of the default `MissedTickBehavior::Burst` for the ticker in question. [#323](https://github.com/webrtc-rs/webrtc/pull/323) by [@k0nserv](https://github.com/k0nserv).
* Don't generate empty TWCC packets that libWebRTC will ignore. [#324](https://github.com/webrtc-rs/webrtc/pull/324) by [@k0nserv](https://github.com/k0nserv).
* Increased minimum support rust version to `1.60.0`.
* Increased required `webrtc-util` version to `0.7.0`.

## v0.8.0

* [#14 Don't panic on seqnum rollover](https://github.com/webrtc-rs/interceptor/pull/14) contributed by by [@pthatcher](https://github.com/pthatcher).
* Add stats interceptor. Contributed by [@k0nserv](https://github.com/k0nserv) in [#277](https://github.com/webrtc-rs/webrtc/pull/277/) and [#225](https://github.com/webrtc-rs/webrtc/pull/225).
* Increased min version of `log` dependency to `0.4.16`. [#250 Fix log at ^0.4.16 to make tests compile](https://github.com/webrtc-rs/webrtc/pull/250) by [@k0nserv](https://github.com/k0nserv).

## Prior to 0.8.0

Before 0.8.0 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/interceptor/releases).

