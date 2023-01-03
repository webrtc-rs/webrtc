# data-channels-flow-control

This example demonstrates how to use the following property / methods.

* pub async fn buffered_amount(&self) -> usize
* pub async fn set_buffered_amount_low_threshold(&self, th: usize)
* pub async fn buffered_amount_low_threshold(&self) -> usize
* pub async fn on_buffered_amount_low(&self, f: OnBufferedAmountLowFn)

These methods are equivalent to that of JavaScript WebRTC API.
See <https://developer.mozilla.org/en-US/docs/Web/API/RTCDataChannel> for more details.

## When do we need it?

Send or SendText methods are called on DataChannel to send data to the connected peer.
The methods return immediately, but it does not mean the data was actually sent onto
the wire. Instead, it is queued in a buffer until it actually gets sent out to the wire.

When you have a large amount of data to send, it is an application's responsibility to
control the buffered amount in order not to indefinitely grow the buffer size to eventually
exhaust the memory.

The rate you wish to send data might be much higher than the rate the data channel can
actually send to the peer over the Internet. The above properties/methods help your
application to pace the amount of data to be pushed into the data channel.

## How to run the example code

The demo code implements two endpoints (requester and responder) in it.

```plain
                        signaling messages
           +----------------------------------------+
           |                                        |
           v                                        v
   +---------------+                        +---------------+
   |               |          data          |               |
   |   requester   |----------------------->|   responder   |
   |:PeerConnection|                        |:PeerConnection|
   +---------------+                        +---------------+
```

First requester and responder will exchange signaling message to establish a peer-to-peer
connection, and data channel (label: "data").

Once the data channel is successfully opened, requester will start sending a series of
1024-byte packets to responder, until you kill the process by Ctrl+С.

Here's how to run the code:

```shell
$ cargo run --release --example data-channels-flow-control
    Finished release [optimized] target(s) in 0.36s
     Running `target\release\examples\data-channels-flow-control.exe`

Throughput is about 127.060 Mbps
Throughput is about 122.091 Mbps
Throughput is about 120.630 Mbps
Throughput is about 120.105 Mbps
Throughput is about 119.873 Mbps
Throughput is about 118.890 Mbps
Throughput is about 118.525 Mbps
Throughput is about 118.614 Mbps
```
