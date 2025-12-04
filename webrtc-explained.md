# WebRTC Core Concepts

Below are the core components you see in WebRTC libraries (including Rust implementations like webrtc-rs).

## Media
Handles audio and video streams:
- microphone
- webcam
- screen share
- encoding/decoding (Opus, VP8, H264, etc.)
- track pipelines
- jitter buffers

Represents the actual media content.

## Interceptor
Middleware for packet flow in WebRTC.

Used for:
- congestion control
- retransmissions
- analytics
- logging
- bitrate adaptation
- simulcast filters

## Data
Refers to WebRTC Data Channels.

Supports:
- text and binary
- reliable or unreliable delivery
- ordered or unordered delivery

Used for chat, game sync, file transfer, etc.

## RTP (Real-time Transport Protocol)
Packet format for real-time media transmission.

Includes:
- timestamps
- sequence numbers
- SSRC identifiers

Carries media data.

## RTCP (RTP Control Protocol)
Feedback channel for RTP.

Reports:
- packet loss
- jitter
- round trip time
- bandwidth
- keyframe requests

## SRTP (Secure RTP)
Encrypted RTP for media.

Uses keys negotiated via DTLS.

## SCTP (Stream Control Transmission Protocol)
Protocol used by WebRTC Data Channels.

Provides:
- multiple streams
- partial reliability
- no head-of-line blocking

Runs inside DTLS.

## DTLS (Datagram TLS)
TLS over UDP.

Used for:
- verifying peer identity
- negotiating encryption keys
- securing SCTP
- securing SRTP

## mDNS
Masks local LAN IP addresses for privacy (e.g. *.local hostnames).

## STUN
Discovers:
- public IP
- public port
- NAT mappings

Used for peer-to-peer connectivity.

## TURN
Relay server fallback when direct P2P isn't possible.

Used in restrictive NAT environments.

## ICE (Interactive Connectivity Establishment)
System that tests multiple network paths and selects the best one.

Uses:
- host candidates
- STUN candidates
- TURN relay candidates

Continuously monitors connectivity.

## SDP (Session Description Protocol)
Text-based connection description used for signaling.

Contains:
- codecs
- ICE credentials
- DTLS fingerprints
- media types
- transport configs

Used in offers and answers.

## Util
Shared helper utilities.

Commonly includes:
- random generators
- buffer helpers
- parsers
- timers
- UUID helpers

