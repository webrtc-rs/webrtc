# webrtc-ice changelog

## Unreleased

### Breaking changes

* remove non used `MulticastDnsMode::Unspecified` variant [#404](https://github.com/webrtc-rs/webrtc/pull/404):

## v0.9.0

* Increased minimum support rust version to `1.60.0`.

### Breaking changes

* Make functions non-async [#338](https://github.com/webrtc-rs/webrtc/pull/338):
  - `Agent`:
    - `get_bytes_received`;
    - `get_bytes_sent`;
    - `on_connection_state_change`;
    - `on_selected_candidate_pair_change`;
    - `on_candidate`;
    - `add_remote_candidate`;
    - `gather_candidates`.
  - `unmarshal_candidate`;
  - `CandidateHostConfig::new_candidate_host`;
  - `CandidatePeerReflexiveConfig::new_candidate_peer_reflexive`;
  - `CandidateRelayConfig::new_candidate_relay`;
  - `CandidateServerReflexiveConfig::new_candidate_server_reflexive`;
  - `Candidate`:
    - `addr`;
    - `set_ip`.

## v0.8.2

* Add IP filter to ICE `AgentConfig` [#306](https://github.com/webrtc-rs/webrtc/pull/306) and [#318](https://github.com/webrtc-rs/webrtc/pull/318).
* Add `rust-version` at 1.57.0 to `Cargo.toml`. This was already the minimum version so does not constitute a change.

## v0.8.1

This release was released in error and contains no changes from 0.8.0.

## v0.8.0

* Increased min version of `log` dependency to `0.4.16`. [#250 Fix log at ^0.4.16 to make tests compile](https://github.com/webrtc-rs/webrtc/pull/250) by [@k0nserv](https://github.com/k0nserv).
* Increased serde's minimum version to 1.0.102 [#243 Fixes for cargo minimal-versions](https://github.com/webrtc-rs/webrtc/pull/243) contributed by [algesten](https://github.com/algesten)

## Prior to 0.8.0

Before 0.8.0 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/ice/releases).
