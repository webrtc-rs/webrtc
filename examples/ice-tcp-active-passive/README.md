# ICE TCP Active/Passive Example

This example demonstrates Rust-to-Rust WebRTC connectivity over TCP (RFC 4571) using the async `webrtc` API. It showcases the difference between active and passive candidate pairing.

## About ICE TCP Types

ICE over TCP defines three connection types (RFC 6544):

- **Passive**: Listens for incoming TCP connections (like a server)
- **Active**: Initiates outgoing TCP connections (like a client)
- **Simultaneous-Open (S-O)**: Both sides attempt connections simultaneously

This example demonstrates **Active** mode on the offering side and **Passive** mode on the answering side.

## How It Works

1. **Answer side** (TCP passive):
   - Configures the `PeerConnection` with a passive candidate listening on a TCP port (default: `8443`).
   - Serves an HTTP signaling server on port `60000` to receive the SDP offer.
   - Sets the remote description, generates an answer, and returns it.

2. **Offer side** (TCP active):
   - Configures the `PeerConnection` with an active candidate (bound to an ephemeral port).
   - The active candidate generates a candidate with port `9` (discard port) for SDP signaling.
   - Generates an SDP offer and POSTs it to the answerer's HTTP signaling server.
   - Sets the answer returned by the passive side.
   - Initiates an outbound TCP connection to the answerer's passive port (`8443`).

3. **Data Channel Exchange**:
   - Once connected, both sides establish the data channel and exchange periodic timestamp messages every 3 seconds.

## Running the Example

### 1. Run the Answer Side (Passive)
From the root of the project, run:

```bash
cargo run --example ice-tcp-passive-answer
```

### 2. Run the Offer Side (Active)
In a separate terminal, run:

```bash
cargo run --example ice-tcp-active-offer
```

For detailed logging, append the `--debug` flag:

```bash
cargo run --example ice-tcp-passive-answer -- --debug
cargo run --example ice-tcp-active-offer -- --debug
```
