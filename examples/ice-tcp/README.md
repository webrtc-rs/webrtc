# ICE TCP Example

This example demonstrates how to establish a WebRTC connection exclusively over TCP (RFC 4571) using the async `webrtc` API.

## About ICE TCP

ICE TCP is useful when UDP traffic is blocked by restrictive firewalls. WebRTC normally uses UDP for media and data transport, but can fall back to TCP when necessary.

This example showcases:
- Configuring the `PeerConnection` to only gather and listen on TCP candidates using `.with_tcp_addrs()` and leaving UDP addresses empty.
- Customizing the DTLS role via `SettingEngine::set_answering_dtls_role(RTCDtlsRole::Client)` to act as the DTLS client when answering the connection.
- Serving a local web interface using `hyper` to serve static pages and handle SDP signaling.
- Establishing a `DataChannel` over pure TCP and exchanging periodic messages.

## How It Works

1. The server starts an HTTP server on port `8080` and configures a WebRTC TCP listener on port `8443`.
2. When a browser opens `http://localhost:8080`, it generates an SDP offer with both TCP and UDP candidates.
3. The browser sends this SDP offer to the server via an HTTP `POST` to `/doSignaling`.
4. The server configures the `PeerConnection`, sets the remote offer, and generates an answer.
5. The server waits for local ICE gathering to complete (disabling trickle ICE) so that the answer includes the server's TCP passive candidate.
6. The server responds with the local SDP answer.
7. The browser sets the answer and initiates an active TCP connection to the server's listener on port `8443`.
8. Once connected, a `DataChannel` is established, and the server sends timestamp messages every 3 seconds.

## Running the Example

### 1. Run the Server
From the root of the project, run:

```bash
cargo run --example ice-tcp
```

For detailed trace logging, use:

```bash
cargo run --example ice-tcp -- --debug
```

### 2. Access the Web UI
Open your browser and navigate to:

```
http://localhost:8080
```

You will see:
- Real-time updates of the ICE connection state.
- Inbound messages received from the server over the established TCP WebRTC data channel.
