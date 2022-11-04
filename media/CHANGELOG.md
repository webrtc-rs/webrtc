# webrtc-media changelog

## Unreleased

* Improve handling of padding packets in `SampleBuiler`. Prior to this `SampleBuilder` would sometimes, incorrectly, drop packets that carry media when they appeared adjacent to runs of padding packets. Contributed by [@k0nserv](https://github.com/k0nserv) in [#309](https://github.com/webrtc-rs/webrtc/pull/309)
* Increased minimum support rust version to `1.60.0`.

### Breaking

* Introduced a new field in `Sample`, `prev_padding_packets`, that reflects the number of observed padding only packets while building the Sample. This can be use to differentiate inconsequential padding packets being dropped from those carrying media. Contributed by [@k0nserv](https://github.com/k0nserv) in [#303](https://github.com/webrtc-rs/webrtc/pull/303).

## v0.4.7

* Bumped util dependency to `0.6.0`.
* Bumped rtp dependency to `0.6.0`.


## Prior to 0.4.7

Before 0.4.7 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/media/releases).
