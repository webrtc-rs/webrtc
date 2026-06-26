# trickle-ice

`trickle-ice` demonstrates async `webrtc` Trickle ICE with host, server reflexive (STUN), and relay (TURN) candidates.

It uses the async peer-connection stack to gather ICE candidates depending on the enabled flags, and trickles them to the browser while the connection is being established.

## Run

From the repository root, run with default settings (Host candidates only):

```bash
cargo run --example trickle-ice
```

Enable only specific candidate types:

```bash
# Only Host candidates
cargo run --example trickle-ice -- --enable-host

# Only STUN Server Reflexive candidates
cargo run --example trickle-ice -- --enable-srflx

# Only TURN Relay candidates
# Requires a TURN server running at 127.0.0.1:3478 with credentials user=pass
cargo run --example trickle-ice -- --enable-relay

# Host + STUN
cargo run --example trickle-ice -- --enable-host --enable-srflx
```

Customize STUN or TURN settings:

```bash
cargo run --example trickle-ice -- --enable-srflx --stun-server stun.l.google.com:19302

cargo run --example trickle-ice -- --enable-relay --turn-host 127.0.0.1 --turn-port 3478 --turn-user user=pass
```

With debug logging:

```bash
cargo run --example trickle-ice -- --debug
```

Then open [http://localhost:8080](http://localhost:8080) and click **Start**.
