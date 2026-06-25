# trickle-ice-srflx

`trickle-ice-srflx` demonstrates async `webrtc` trickle ICE with STUN-backed
server reflexive local candidates.

## Instructions

Run:

```bash
cargo run --example trickle-ice-srflx
```

Or with debug logging:

```bash
cargo run --example trickle-ice-srflx -- --debug
```

Then open [http://localhost:8080](http://localhost:8080) and click `Start`.

The example sends the SDP answer immediately and then trickles host and srflx
ICE candidates to the browser as they become available.
