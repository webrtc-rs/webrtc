<h1 align="center">
  Examples
</h1>

All examples are ported from [Pion](https://github.com/pion/webrtc/tree/master/examples#readme). Please
check [Pion Examples](https://github.com/pion/webrtc/tree/master/examples#readme) for more details:

### Data Channel API

- ✅ [Data Channels](data-channels): The data-channels example shows how you can send/recv DataChannel messages from a
  web browser.
- ✅ [Data Channels Create](data-channels-create): Example data-channels-create shows how you can send/recv DataChannel
  messages from a web browser. The difference with the data-channels example is that the data channel is initialized
  from the server side in this example.
- ✅ [Data Channels Close](data-channels-close): Example data-channels-close is a variant of data-channels that allow
  playing with the life cycle of data channels.
- ✅ [Data Channels Flow Control](data-channels-flow-control): Example data-channels-flow-control shows how to use flow
  control.
- ✅ [Data Channels Offer Answer](data-channels-offer-answer): Example offer-answer is an example of two webrtc-rs
  instances communicating directly!
- ✅ [Data Channels Simple](data-channels-simple): Simple example of a WebRTC DataChannel using it as the signaling
  server.

### Media API

- ✅ [Reflect](reflect): The reflect example demonstrates how to have webrtc-rs send back to the user exactly what it
  receives using the same PeerConnection.
- ✅ [Play from Disk VPx](play-from-disk-vpx): The play-from-disk-vpx example demonstrates how to send VP8/VP9 video to
  your browser from a file saved to disk.
- ✅ [Play from Disk H26x](play-from-disk-h26x): The play-from-disk-h26x example demonstrates how to send H264/H265 video
  to your browser from a file saved to disk.
- ✅ [Play from Disk Renegotiation](play-from-disk-renegotiation): The play-from-disk-renegotiation example is an
  extension of the play-from-disk example, but demonstrates how you can add/remove video tracks from an already
  negotiated PeerConnection.
- ✅ [Save to Disk VPx](save-to-disk-vpx): The save-to-disk-vpx example shows how to record your webcam and save the
  footage (VP8/VP9 for video, Opus for audio) to disk on the server side.
- ✅ [Save to Disk H26x](save-to-disk-h26x): The save-to-disk-h26x example shows how to record your webcam and save the
  footage (H264/H265 for video, Opus for audio) to disk on the server side.
- ✅ [Insertable Streams](insertable-streams): The insertable-streams example demonstrates how webrtc-rs can be used to
  send E2E encrypted video and decrypt via insertable streams in the browser.
- ✅ [Broadcast](broadcast): The broadcast example demonstrates how to broadcast a video to multiple peers. A
  broadcaster uploads the video once and the server forwards it to all other peers.
- ✅ [RTP Forwarder](rtp-forwarder): The rtp-forwarder example demonstrates how to forward your audio/video streams
  using RTP.
- ✅ [RTP to WebRTC](rtp-to-webrtc): The rtp-to-webrtc example demonstrates how to take RTP packets sent to a webrtc-rs
  process into your browser.
- ✅ [Simulcast](simulcast): The simulcast example demonstrates how to accept and demux 1 Track that contains 3
  Simulcast streams. It then returns the media as 3 independent Tracks back to the sender.
- ✅ [Swap Tracks](swap-tracks): The swap-tracks demonstrates how to swap multiple incoming tracks on a single outgoing
  track.
- ✅ [RTCP Processing](rtcp-processing): The rtcp-processing example demonstrates how to create a custom
  RtcpForwarderInterceptor using the derive macros. This allows access to media statistics and control information.
- ✅ [Save to Disk AV1](save-to-disk-av1): The save-to-disk-av1 is a simple application that shows how to save a video to
  disk using AV1.
- ✅ [Play from Disk Playlist Control](play-from-disk-playlist-control): Streams Opus pages from multi or single track
  Ogg containers, exposes the playlist over an SCTP DataChannel, and lets the browser hop between tracks while showing
  artist/title metadata parsed from OpusTags.

### Miscellaneous

- ✅ [mDNS Query and Gather](mdns-query-and-gather) Example mdns-query-and-gather demonstrates webrtc-rs hides local ip
  with mDNS.
- ✅ [ICE Restart](ice-restart): The ice-restart demonstrates webrtc-rs ICE Restart abilities.
- ✅ [Trickle ICE Host](trickle-ice-host) Example demonstrates WebRTC's Trickle ICE APIs to add Host type local
  candidate.
- ✅ [Trickle ICE ServerReflexive](trickle-ice-srflx): Example demonstrates how to add ServerReflexive (STUN)
  type local candidate.
- ✅ [Trickle ICE Relay](trickle-ice-relay): Example demonstrates how to add Relay (TURN)
  type local candidate.
- ✅ [Trickle ICE](trickle-ice) Example trickle-ice demonstrates the comprehensive Trickle ICE APIs with all three types
  of ICE candidates. This is important to use since it allows ICE Gathering and Connecting to happen concurrently.
- ✅ [ICE TCP](ice-tcp) Example ice-tcp demonstrates how a WebRTC connection can be made over TCP instead of UDP. By
  default, webrtc-rs only does UDP. webrtc-rs can be configured to use a TCP port with passive mode.
- ✅ [ICE TCP Active-Passive](ice-tcp-active-passive) Example ice-tcp-active-passive demonstrates RTC's ICE TCP active
  mode abilities.
- ✅ [Stats](stats): Stats demonstrates how to use the webrtc-stats implementation provided by WebRTC-rs.

### TODO

- 🚧 [Play from Disk FEC](TODO): The play-from-disk-fec demonstrates how to use forward error correction (FlexFEC-03)
  while sending video to your Chrome-based browser from files saved to disk. The example is designed to drop 40% of the
  media packets, but browser will recover them using the FEC packets and the delivered packets.

### Not Applicable

- [x] [ICE Single Port](N/A) This example doesn't apply to sansio RTC because sansio RTC is I/O-free by design: The
  library never creates or manages sockets. Your application creates UDP sockets and feeds data to handle_read(). Port
  multiplexing is already the application's responsibility.
- [x] [ICE Proxy](N/A) This example doesn't apply to sansio RTC too, since Sansio RTC is I/O-free - it never creates
  network connections. The application is responsible for all I/O.
- [x] [Data Channels WHIP WHEP](N/A): This example doesn't apply to sansio RTC, since it demonstrates a WHIP/WHEP-like
  implementation.
- [x] [WHIP WHEP](N/A): This example doesn't apply to sansio RTC, since it demonstrates using WHIP and WHEP.
