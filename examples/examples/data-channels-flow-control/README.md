# data-channels-flow-control
This example demonstrates how to use the following property / methods.

* pub async fn buffered_amount(&self) -> usize
* pub async fn set_buffered_amount_low_threshold(&self, th: usize) 
* pub async fn buffered_amount_low_threshold(&self) -> usize
* pub async fn on_buffered_amount_low(&self, f: OnBufferedAmountLowFn)

These methods are equivalent to that of JavaScript WebRTC API.
See https://developer.mozilla.org/en-US/docs/Web/API/RTCDataChannel for more details.

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

The demo code implements two endpoints (offer_pc and answer_pc) in it.

```
                        signaling messages
           +----------------------------------------+
           |                                        |
           v                                        v
   +---------------+                        +---------------+
   |               |          data          |               |
   |    offer_pc    |----------------------->|    answer_pc   |
   |:PeerConnection|                        |:PeerConnection|
   +---------------+                        +---------------+
```

First offer_pc and answer_pc will exchange signaling message to establish a peer-to-peer
connection, and data channel (label: "data").

Once the data channel is successfully opened, offer_pc will start sending a series of
1024-byte packets to answer_pc as fast as it can, until you kill the process by Ctrl-c.


Here's how to run the code.

At the root of the example:
```
$ cargo run
Peer Connection State has changed: connected (offerer)
Peer Connection State has changed: connected (answerer)
OnOpen: data-1. Start sending a series of 1024-byte packets as fast as it can
OnOpen: data-1. Start receiving data
Throughput: 12.990 Mbps
Throughput: 13.698 Mbps
Throughput: 13.559 Mbps
Throughput: 13.345 Mbps
Throughput: 13.565 Mbps
 :
```
