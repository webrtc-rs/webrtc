# webrtc-sctp changelog

## Unreleased

* Allow partial reads [\#304](https://github.com/webrtc-rs/webrtc/pull/304)
  `read` no longer returns `ErrShortBuffer` if the buffer is too small to fit a
  whole message. The buffer will be filled up to its size and the rest of the
  msg will be returned upon the next call to `read`.

## v0.6.1

* Increased min verison of `log` dependency to `0.4.16`. [#250 Fix log at ^0.4.16 to make tests compile](https://github.com/webrtc-rs/webrtc/pull/250) by [@k0nserv](https://github.com/k0nserv).
* [#245 Fix incorrect chunk type Display for CWR](https://github.com/webrtc-rs/webrtc/pull/245) by [@k0nserv](https://github.com/k0nserv).

## Prior to 0.6.1

Before 0.6.1 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/sctp/releases).

