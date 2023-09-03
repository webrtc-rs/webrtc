# rtp changelog

## Unreleased

## v0.6.8

* Increased minimum support rust version to `1.60.0`.
* Adds a new generic header extensions type `rtp::extension::HeaderExtension` which allows abstracting over all known extensions as well as custom extensions. [#336](https://github.com/webrtc-rs/webrtc/pull/336) by [@k0nserv](https://github.com/k0nserv).
* Added video orientation(`urn:3gpp:video-orientation`) extension support. [#331](https://github.com/webrtc-rs/webrtc/pull/331) by [@algesten](https://github.com/algesten).
* Allow RTP extensions to be serialized and deserialized via serder. [#332](https://github.com/webrtc-rs/webrtc/pull/332) by [@algesten](https://github.com/algesten).
* Increased required `webrtc-util` version to `0.7.0`.

## v0.6.7

* Bumped util dependency to `0.6.0`.

## Prior to 0.6.7

Before 0.6.7 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/rtp/releases).

