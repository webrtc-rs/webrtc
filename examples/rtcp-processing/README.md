# rtcp-processing

`rtcp-processing` demonstrates the async WebRTC API for processing RTCP packets.

## What it shows

1. Building a `PeerConnection` with the async `PeerConnectionBuilder` pattern used by the other async examples.
2. Adding a custom `RtcpForwarderInterceptor` that narrows inbound RTCP to keyframe requests — PLI and FIR — and marks
   those with `Attribute::DeliverToApplication` so they reach the application instead of stopping at the end of the chain.
4. Receiving those requests on the async side through `TrackRemoteEvent::OnRtcpPacket`.
5. Printing their headers and human-readable bodies as media flows.

## Why a packet has to be marked

By default, inbound RTCP is consumed inside the interceptor chain for reports, NACK handling, congestion control, and
similar logic — it is control traffic the interceptors act on, not media the application asked for. An interceptor that
attaches `Attribute::DeliverToApplication` to a packet vouches for it, and that packet arrives alongside the media.

Marking is the mechanism because a copy cannot outrun the end of the chain: a chain is a flat list, and what an
interceptor emits rejoins that list *behind* itself, where the stage that ends the inbound RTCP path is still ahead of
it. A marked packet finishes the walk normally and that stage reads the mark.

Marking also keeps the judgement per-packet, which a chain-wide switch could not: this example vouches for keyframe
requests and lets the receiver reports its own chain is acting on stop where they always did. This example mirrors the
sansio `rtc/examples/rtcp-processing` example.

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

Without an interceptor marking packets, `TrackRemoteEvent::OnRtcpPacket` is not emitted at all: the chain ends the
inbound RTCP path before the application. With the `RtcpForwarderInterceptor`, only PLI and FIR arrive — this peer is
receive-only in the jsfiddle above, so it is the one *sending* keyframe requests and will print nothing until something
downstream asks it for one.

To see every inbound RTCP packet instead, widen what the interceptor vouches for rather than removing it: mark each
`Packet::Rtcp` unconditionally in its `handle_read`. Removing the line altogether leaves nothing to mark them, and the
application sees no RTCP at all.
