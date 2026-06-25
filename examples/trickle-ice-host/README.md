# trickle-ice-host

`trickle-ice-host` demonstrates async `webrtc` trickle ICE with host local
candidates.

## Instructions

Run:

```bash
cargo run --example trickle-ice-host
```

Or with debug logging:

```bash
cargo run --example trickle-ice-host -- --debug
```

Then open [http://localhost:8080](http://localhost:8080) and click `Start`.

The example sends the SDP answer immediately and trickles host ICE candidates to
the browser as they are gathered.
