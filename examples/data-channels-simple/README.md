# WebRTC DataChannel Example in Rust

This is a minimal example of a **WebRTC DataChannel** using **Rust (sansio RTC)** as the signaling server.

`hyper` v0.14 requires a Tokio runtime (it uses `tokio::net::TcpListener` internally via `Server::bind`). When we run
under `runtime-smol`, there's no Tokio
reactor, so `hyper` panics. Therefore, this example must be ran in default runtime-tokio.

## Features

- Rust HTTP server for signaling (using hyper)
- Browser-based DataChannel
- ICE candidate exchange
- Real-time messaging between browser and Rust server

## Usage

1. Run the server:

```bash
cargo run --example data-channels-simple
```

2. Open browser at http://localhost:8080

3. Send messages via DataChannel and see them in terminal & browser logs.

## How It Works

1. The browser creates a DataChannel and generates an SDP offer
2. The offer is sent to the Rust server via HTTP POST to `/offer`
3. The server creates a PeerConnection, processes the offer, and returns an answer
4. The browser sets the answer as remote description
5. ICE candidates are exchanged via HTTP POST to `/candidate`
6. Once connected, messages can be sent bidirectionally through the DataChannel

## Architecture

```text
┌─────────────┐                     ┌──────────────────┐
│   Browser   │                     │   Rust Server    │
│             │                     │                  │
│ DataChannel │◄──── WebRTC ───────►│ RTCPeerConnection|
│             │      (UDP)          │                  │
│  HTTP POST  │────────────────────►│  /offer          │
│  HTTP POST  │────────────────────►│  /candidate      │
└─────────────┘                     └──────────────────┘
```

## API Endpoints

| Endpoint     | Method | Description                      |
|--------------|--------|----------------------------------|
| `/`          | GET    | Serve demo HTML page             |
| `/offer`     | POST   | Receive SDP offer, return answer |
| `/candidate` | POST   | Receive ICE candidate            |
