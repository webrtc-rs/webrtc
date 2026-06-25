# rtcp-processing

`rtcp-processing` demonstrates the async WebRTC API for processing RTCP packets.

## What it shows

1. Building a `PeerConnection` with the async `PeerConnectionBuilder` pattern used by the other async examples.
2. Registering a custom `RtcpForwarderInterceptor` so RTCP is forwarded out of the interceptor chain.
3. Receiving forwarded RTCP on the async side through `TrackRemoteEvent::OnRtcpPacket`.
4. Printing RTCP packet headers and human-readable packet bodies as media flows.

## Why the custom interceptor is needed

By default, RTCP is consumed inside the interceptor chain for reports, NACK handling, congestion control, and similar
logic.
This example mirrors the sansio `rtc/examples/examples/rtcp-processing` example by adding an outer interceptor that
queues RTCP
for application delivery before passing it down the normal chain.

### Open rtcp-processing example page

[jsfiddle.net](https://jsfiddle.net/zurq6j7x/) you should see two text-areas, 'Start Session' button and 'Copy browser
SessionDescription to clipboard'

## Build

```shell
cargo build --example rtcp-processing
```

## Run

```shell
cargo run --example rtcp-processing
```

## With debug logging

```shell
cargo run --example rtcp-processing -- --debug
```

## Read SDP from a file

```shell
cargo run --example rtcp-processing -- --input-sdp-file offer.txt
```

## Signaling flow

1. Paste a base64-encoded SDP offer from a browser.
2. Copy the printed base64 answer back into the browser.
3. Start sending audio/video from the browser.
4. Watch incoming RTCP packets printed per remote track.

## Async RTCP delivery

With the custom forwarder registered, RTCP arrives through the remote track event loop:

```rust
while let Some(evt) = track.poll().await {
if let TrackRemoteEvent::OnRtcpPacket(rtcp_packets) = evt {
for packet in rtcp_packets {
let header = packet.header();
println! ("Type: {:?}", header.packet_type);
println! ("{packet}");
}
}
}
```

Without the custom `RtcpForwarderInterceptor`, `TrackRemoteEvent::OnRtcpPacket` will not be emitted for normal inbound
RTCP.
