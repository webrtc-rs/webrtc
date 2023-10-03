# webrtc-util changelog

## v0.7.0

### Breaking changes

* Make functions non-async [#338](https://github.com/webrtc-rs/webrtc/pull/338):
    - `Bridge`:
        - `drop_next_nwrites`;
        - `reorder_next_nwrites`.
    - `Conn`:
        - `local_addr`;
        - `remote_addr`.


## v0.6.0

* Increase min version of `log` dependency to `0.4.16`. [#250 Fix log at ^0.4.16 to make tests compile](https://github.com/webrtc-rs/webrtc/pull/250) by [@k0nserv](https://github.com/k0nserv).
* Increased minimum support rust version to `1.60.0`.

## Prior to 0.6.0

Before 0.6.0 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/util/releases).

