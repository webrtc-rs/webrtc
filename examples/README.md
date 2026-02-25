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
- ✅ [Data Channels Offer Answer](offer-answer): Example offer-answer is an example of two webrtc-rs or pion instances
  communicating directly!
- ✅ [Data Channels Simple](data-channels-simple): Simple example of a WebRTC DataChannel using it as the signaling
  server.

### Media API

- ✅ [Reflect](reflect): The reflect example demonstrates how to have webrtc-rs send back to the user exactly what it
  receives using the same PeerConnection.
- ✅ [Play from Disk VPx](play-from-disk-vpx): The play-from-disk-vpx example demonstrates how to send VP8/VP9 video to
  your browser from a file saved to disk.
- ✅ [Play from Disk H26x](play-from-disk-h26x): The play-from-disk-h26x example demonstrates how to send H26x video to
  your browser from a file saved to disk.
- ✅ [Play from Disk Renegotiation](play-from-disk-renegotiation): The play-from-disk-renegotiation example is an
  extension of the play-from-disk example, but demonstrates how you can add/remove video tracks from an already
  negotiated PeerConnection.
- 🚧 [Insertable Streams](insertable-streams): The insertable-streams example demonstrates how webrtc-rs can be used to
  send E2E encrypted video and decrypt via insertable streams in the browser.
- 🚧 [Save to Disk VPx](save-to-disk-vpx): The save-to-disk-vpx example shows how to record your webcam and save the
  footage (VP8/VP9 for video, Opus for audio) to disk on the server side.
- 🚧 [Save to Disk H26x](save-to-disk-h26x): The save-to-disk-h26x example shows how to record your webcam and save the
  footage (H26x for video, Opus for audio) to disk on the server side.
- 🚧 [Broadcast](broadcast): The broadcast example demonstrates how to broadcast a video to multiple peers. A
  broadcaster uploads the video once and the server forwards it to all other peers.
- 🚧 [RTP Forwarder](rtp-forwarder): The rtp-forwarder example demonstrates how to forward your audio/video streams
  using RTP.
- 🚧 [RTP to WebRTC](rtp-to-webrtc): The rtp-to-webrtc example demonstrates how to take RTP packets sent to a webrtc-rs
  process into your browser.
- 🚧 [Simulcast](simulcast): The simulcast example demonstrates how to accept and demux 1 Track that contains 3
  Simulcast streams. It then returns the media as 3 independent Tracks back to the sender.
- 🚧 [Swap Tracks](swap-tracks): The swap-tracks demonstrates how to swap multiple incoming tracks on a single outgoing
  track.

### Miscellaneous

- ✅ [ICE Restart](ice-restart): The ice-restart demonstrates webrtc-rs ICE Restart abilities.