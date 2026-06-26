# trickle-ice-relay

`trickle-ice-relay` demonstrates async `webrtc` Trickle ICE with **TURN relay-only** local candidates.

It uses the async peer-connection stack to gather relay candidates from a configured TURN server, then trickles them to
the browser while the answer is being established.

## Prerequisites

You need a TURN server running. You can use either:

### Option 1: webrtc-rs/webrtc v0.17.x branch turn server

From the webrtc-rs/webrtc v0.17.x branch repository:

```bash
RUST_LOG=trace cargo run --color=always --package turn --example turn_server_udp -- --public-ip <browser-reachable-ip> --users user=pass
```

### Option 2: pion/turn server

From the pion/turn repository:

```bash
./simple -public-ip <browser-reachable-ip> -users user=pass
```

## Run

From the repository root:

```bash
cargo run --example trickle-ice-relay
```

With explicit TURN settings:

```bash
cargo run --example trickle-ice-relay -- --turn-host <browser-reachable-ip> --turn-port 3478 --turn-user user=pass
```

With debug logging:

```bash
cargo run --example trickle-ice-relay -- --debug
```

Then open [http://localhost:8080](http://localhost:8080) and click **Start**.
