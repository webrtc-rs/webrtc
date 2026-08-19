# rtcp-processing

`rtcp-processing` demonstrates the async WebRTC API for processing RTCP packets.

## What it shows

1. Building a `PeerConnection` with the async `PeerConnectionBuilder` pattern used by the other async examples.
2. Calling `Registry::with_rtcp_readable()` so inbound RTCP reaches the application instead of stopping at the end of the chain.
3. Adding a custom `RtcpForwarderInterceptor` that narrows what the application sees to keyframe requests — PLI and FIR — and drops the rest.
4. Receiving those requests on the async side through `TrackRemoteEvent::OnRtcpPacket`.
5. Printing their headers and human-readable bodies as media flows.

## Why the chain has to be asked

By default, inbound RTCP is consumed inside the interceptor chain for reports, NACK handling, congestion control, and
similar logic — it is control traffic the interceptors act on, not media the application asked for.
`Registry::with_rtcp_readable()` says otherwise, and then RTCP arrives alongside the media.

It has to be asked for when the chain is built rather than arranged by an interceptor of your own: a chain is a flat
list, and what an interceptor emits rejoins that list *behind* itself, where the stage that ends the inbound RTCP path
is still ahead of it. This example mirrors the sansio `rtc/examples/rtcp-processing` example.

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

Without `Registry::with_rtcp_readable()`, `TrackRemoteEvent::OnRtcpPacket` is not emitted at all: the chain ends the
inbound RTCP path before the application. With it, plus the `RtcpForwarderInterceptor`, only PLI and FIR arrive —
this peer is receive-only in the jsfiddle above, so it is the one *sending* keyframe requests and will print nothing
until something downstream asks it for one. Drop the `.with(RtcpForwarderBuilder::new().build())` line to see every
inbound RTCP packet instead.
