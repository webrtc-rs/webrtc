# stats

stats demonstrates how to use the [webrtc-stats](https://www.w3.org/TR/webrtc-stats/) implementation provided by WebRTC-rs.

This API gives you access to statistical information about a PeerConnection. This can help you understand what is happening during a session and why.

## Instructions

### Open stats example page

Go to [jsfiddle.net](https://jsfiddle.net/s179hacu/). You should see your Webcam, two text-areas and two buttons: `Copy browser SDP to clipboard`, `Start Session`.

### Run stats, with your browser's SessionDescription as stdin

In the jsfiddle, the top textarea contains your browser's Session Description. Press `Copy browser SDP to clipboard` or copy the base64 string manually. We will use this value in the next step.

#### Linux/macOS

Run:
```bash
echo $BROWSER_SDP | cargo run --example stats
```

#### Windows

1. Paste the SessionDescription into a file.
2. Run:
```cmd
cargo run --example stats < my_file
```

### Input stats' SessionDescription into your browser

Copy the base64 string that `stats` just emitted and paste it into the second text area in the browser.

### Hit 'Start Session' in jsfiddle

The `stats` program will now print WebRTC statistics every 5 seconds, including InboundRTPStreamStats for each incoming stream and Remote IP+Ports.

You will see the following in your console:

```
=== WebRTC Stats ===
{
  "data_channels_closed": 0,
  "data_channels_opened": 0
}

Inbound RTP Stats for: video/vp8
{
  "packets_received": 1255,
  "packets_lost": 0,
  "jitter": 588.9559641717999,
  "bytes_received": 1361125,
  "track_identifier": "video-track",
  "ssrc": 1234567890,
  ...
}

Inbound RTP Stats for: audio/opus
{
  "packets_received": 2450,
  "packets_lost": 0,
  "jitter": 12.5,
  "bytes_received": 245000,
  "track_identifier": "audio-track",
  "ssrc": 987654321,
  ...
}

Remote Candidate:
{
  "address": "192.168.1.93",
  "port": 59239,
  "candidate_type": "host",
  ...
}
====================
```

## What it demonstrates

This example demonstrates:
1. How to use the `peer_connection.get_stats()` API to retrieve WebRTC statistics asynchronously.
2. How to access different types of stats from the stats report:
   - Peer connection stats via `report.peer_connection()`
   - Inbound RTP stream stats via `report.inbound_rtp_streams()`
   - ICE candidate stats via `report.iter_by_type()`
3. How to print stats periodically at regular intervals (every 5 seconds) using background async tasks.
