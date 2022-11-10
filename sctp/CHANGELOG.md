# webrtc-sctp changelog

## Unreleased

* Increased minimum support rust version to `1.60.0`.
* Do not loose data in `PollStream::poll_write` [#341](https://github.com/webrtc-rs/webrtc/pull/341).
* `PollStream::poll_shutdown`: make sure to flush any writes before shutting down [#340](https://github.com/webrtc-rs/webrtc/pull/340)
* Fixed a possible bug when adding chunks to pending queue [#345](https://github.com/webrtc-rs/webrtc/pull/345)

## v0.6.1

* Increased min verison of `log` dependency to `0.4.16`. [#250 Fix log at ^0.4.16 to make tests compile](https://github.com/webrtc-rs/webrtc/pull/250) by [@k0nserv](https://github.com/k0nserv).
* [#245 Fix incorrect chunk type Display for CWR](https://github.com/webrtc-rs/webrtc/pull/245) by [@k0nserv](https://github.com/k0nserv).

## Prior to 0.6.1

Before 0.6.1 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/sctp/releases).
