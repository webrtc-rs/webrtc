# webrtc-ice changelog

## Unreleased

## v0.8.1

* Promote agent lock in ice_gather.rs create_agent() to top level of the function to avoid a race condition. [#290 Promote create_agent lock to top of function, to avoid race condition](https://github.com/webrtc-rs/webrtc/pull/290) contributed by [efer-ms](https://github.com/efer-ms)

## v0.8.0

* Increased min verison of `log` dependency to `0.4.16`. [#250 Fix log at ^0.4.16 to make tests compile](https://github.com/webrtc-rs/webrtc/pull/250) by [@k0nserv](https://github.com/k0nserv).
* Incresed serde's minimum version to 1.0.102 [#243 Fixes for cargo minimal-versions](https://github.com/webrtc-rs/webrtc/pull/243) contributed by [algesten](https://github.com/algesten)


## Prior to 0.8.0

Before 0.8.0 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/ice/releases).

