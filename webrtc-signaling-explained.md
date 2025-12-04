# WebRTC Signaling Explained

## What is Signaling?

Signaling is the process where two peers exchange the information required to establish a WebRTC connection.  
It happens *before* any direct peer connection is possible.

This includes exchanging:
- SDP Offer
- SDP Answer
- ICE Candidates


## Why Signaling Is Needed

WebRTC peers cannot discover each other on their own.

They need to:
- agree on codecs, media and datachannel setup
- share network information
- exchange security fingerprints

Signaling provides this exchange.


## What Signaling Is Not

Signaling is **not**:
- part of WebRTC itself
- standardized
- a fixed protocol
- tied to any transport


## Signaling Transport Options

Signaling can use any transport, including:
- WebSockets
- HTTP
- REST
- SSE
- MQTT
- TCP
- UDP
- Redis pub/sub
- anything else


## What Information Gets Exchanged

### SDP (Session Description Protocol)
Contains:
- codecs
- media directions
- ICE usernames/passwords
- DTLS fingerprints
- transport description
- media streams

### ICE Candidates
Contain:
- IP addresses
- ports
- transport protocol
- candidate priority


## Typical Signaling Flow

1. Peer A creates PeerConnection
2. Peer A generates Offer (SDP)
3. Peer A sends Offer to Peer B using the signaling channel
4. Peer B sets remote Offer
5. Peer B generates Answer (SDP)
6. Peer B sends Answer back via signaling channel
7. Both sides exchange ICE Candidates
8. ICE finds a working route
9. Peers connect directly (or through a TURN relay, but still end-to-end encrypted)


## Signaling Server Responsibilities

A signaling server:
- relays Offer
- relays Answer
- relays ICE candidates

It does *not*:
- relay audio/video (RTP)
- relay data channel traffic
- stay involved after connection succeeds


## Server After Connection

Once the WebRTC connection is established:
- the signaling server is no longer required
- peers communicate directly (unless TURN is used when the peers are behind restrictive NAT)


## Simplified Diagram

Peer A ---- Offer/Answer/ICE ----> Signaling Server ---- Offer/Answer/ICE ----> Peer B

Peer A <------------------------------------- P2P ------------------------------------> Peer B


## Summary

- Signaling exists only to bootstrap WebRTC.
- It exchanges Offer, Answer, and ICE candidates.
- It is not part of WebRTC protocol.
- You can use any transport you want.
- Once done, peers can communicate directly.

