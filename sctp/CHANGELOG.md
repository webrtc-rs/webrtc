# webrtc-sctp changelog

## Unreleased

* Use the new algorithm in crc crate for better throughput [#569](https://github.com/webrtc-rs/webrtc/pull/569)

## v0.8.0

* Fix 'attempt to add with overflow' panic in dev profile [#393](https://github.com/webrtc-rs/webrtc/pull/393)
* Limit the bytes in the PendingQueue to avoid packets accumulating there uncontrollably [#367](https://github.com/webrtc-rs/webrtc/pull/367).
* Improve algorithm used to push to pending queue from O(n*log(n)) to O(log(n)) [#365](https://github.com/webrtc-rs/webrtc/pull/365).
* Reuse as many allocations as possible when marshaling [#364](https://github.com/webrtc-rs/webrtc/pull/364).
* The lock for the internal association was contended badly because marshaling was done while still in a critical section and also tokio was scheduling tasks badly [#363](https://github.com/webrtc-rs/webrtc/pull/363).

### Breaking

* Make `sctp::Stream::write` & `sctp::Stream::write_sctp` async again [#367](https://github.com/webrtc-rs/webrtc/pull/367).

## v0.7.0

* Increased minimum support rust version to `1.60.0`.
* Do not loose data in `PollStream::poll_write` [#341](https://github.com/webrtc-rs/webrtc/pull/341).
* `PollStream::poll_shutdown`: make sure to flush any writes before shutting down [#340](https://github.com/webrtc-rs/webrtc/pull/340).
* Fixed a possible bug when adding chunks to pending queue [#345](https://github.com/webrtc-rs/webrtc/pull/345).
* Increased required `webrtc-util` version to `0.7.0`.

### Breaking changes

* Make `Stream::on_buffered_amount_low` function non-async [#338](https://github.com/webrtc-rs/webrtc/pull/338).
* Make `sctp::Stream::write` & `sctp::Stream::write_sctp` sync [#344](https://github.com/webrtc-rs/webrtc/pull/344).

## v0.6.1

* Increased min version of `log` dependency to `0.4.16`. [#250 Fix log at ^0.4.16 to make tests compile](https://github.com/webrtc-rs/webrtc/pull/250) by [@k0nserv](https://github.com/k0nserv).
* [#245 Fix incorrect chunk type Display for CWR](https://github.com/webrtc-rs/webrtc/pull/245) by [@k0nserv](https://github.com/k0nserv).

## Prior to 0.6.1

Before 0.6.1 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/sctp/releases).
