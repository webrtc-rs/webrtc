# rtcp changelog

## Unreleased

## v0.8.0

* Fix over-NACK due not resetting lost_packets bitmask [\#372](https://github.com/webrtc-rs/webrtc/pull/372/).
* Increased minimum support rust version to `1.60.0`.
* Increased required `webrtc-util` version to `0.7.0`.

## v0.7.0

* [#14 Prevent crash in RTCP NACK writing](https://github.com/webrtc-rs/rtcp/pull/14) by [@pthatcher](https://github.com/pthatcher).
* Adds `IntoIterator` for `NackPair` which iterates over all the sequence numbers specified by the `NackPair`. This is similar to `packet_list` but without requiring the allocation of a Vec. Added in [#225 Add RTP Stats to stats report](https://github.com/webrtc-rs/webrtc/pull/225) by [@k0nserv](https://github.com/k0nserv).


## Prior to 0.7.0

Before 0.7.0 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/rtcp/releases).
