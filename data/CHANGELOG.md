# webrtc-data changelog

## Unreleased

* Increased minimum support rust version to `1.60.0`.
* `PollDataChannel::poll_shutdown`: make sure to flush any writes before shutting down [#340](https://github.com/webrtc-rs/webrtc/pull/340)

## 0.5.0

* [#16 [PollDataChannel] reset shutdown_fut future after done](https://github.com/webrtc-rs/data/pull/16) by [@melekes](https://github.com/melekes).
* Increase min verison of `log` dependency to `0.4.16`. [#250 Fix log at ^0.4.16 to make tests compile](https://github.com/webrtc-rs/webrtc/pull/250) by [@k0nserv](https://github.com/k0nserv).

## Prior to 0.4.0

Before 0.4.0 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/data/releases).
