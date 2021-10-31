<h1 align="center">
  Examples
</h1>

All examples are ported from [Pion](https://github.com/pion/webrtc/tree/master/examples#readme). Please check [Pion Examples](https://github.com/pion/webrtc/tree/master/examples#readme) for more details:

#### Media API
- [x] [Reflect](reflect): The reflect example demonstrates how to have webrtc-rs send back to the user exactly what it receives using the same PeerConnection.
- [x] [Play from Disk VPx](play-from-disk-vpx): The play-from-disk-vp8 example demonstrates how to send VP8/VP9 video to your browser from a file saved to disk.
- [x] [Play from Disk H264](play-from-disk-h264): The play-from-disk-h264 example demonstrates how to send H264 video to your browser from a file saved to disk.
- [x] [Play from Disk Renegotiation](play-from-disk-renegotiation): The play-from-disk-renegotiation example is an extension of the play-from-disk example, but demonstrates how you can add/remove video tracks from an already negotiated PeerConnection.
- [x] [Insertable Streams](insertable-streams): The insertable-streams example demonstrates how webrtc-rs can be used to send E2E encrypted video and decrypt via insertable streams in the browser.
- [x] [Save to Disk VPx](save-to-disk-vpx): The save-to-disk example shows how to record your webcam and save the footage (VP8/VP9 for video, Opus for audio) to disk on the server side.
- [x] [Save to Disk H264](save-to-disk-h264): The save-to-disk example shows how to record your webcam and save the footage (H264 for video, Opus for audio) to disk on the server side.
- [x] [Broadcast](broadcast): The broadcast example demonstrates how to broadcast a video to multiple peers. A broadcaster uploads the video once and the server forwards it to all other peers.
- [x] [RTP Forwarder](rtp-forwarder): The rtp-forwarder example demonstrates how to forward your audio/video streams using RTP.
- [x] [RTP to WebRTC](rtp-to-webrtc): The rtp-to-webrtc example demonstrates how to take RTP packets sent to a webrtc-rs process into your browser.
- [x] [Simulcast](simulcast): The simulcast example demonstrates how to accept and demux 1 Track that contains 3 Simulcast streams. It then returns the media as 3 independent Tracks back to the sender.
- [x] [Swap Tracks](swap-tracks): The swap-tracks demonstrates how to swap multiple incoming tracks on a single outgoing track.

#### Data Channel API
- [x] [] [Data Channels](data-channels): The data-channels example shows how you can send/recv DataChannel messages from a web browser.
- [x] [] [Data Channels Create](data-channels-create): Example data-channels-create shows how you can send/recv DataChannel messages from a web browser. The difference with the data-channels example is that the data channel is initialized from the server side in this example.
- [x] [] [Data Channels Close](data-channels-close): Example data-channels-close is a variant of data-channels that allow playing with the life cycle of data channels.
- [x] [] [Data Channels Detach](data-channels-detach): The data-channels-detach example shows how you can send/recv DataChannel messages using the underlying DataChannel implementation directly. This provides a more idiomatic way of interacting with Data Channels.
- [x] [] [Data Channels Detach Create](data-channels-detach-create): Example data-channels-detach-create shows how you can send/recv DataChannel messages using the underlying DataChannel implementation directly. This provides a more idiomatic way of interacting with Data Channels. The difference with the data-channels-detach example is that the data channel is initialized in this example.
- [x] [x] [Data Channels Flow Control](data-channels-flow-control): Example data-channels-flow-control shows how to use flow control.
- [x] [] [ORTC](ortc): Example ortc shows how to use the ORTC API for DataChannel communication.
- [x] [] [Offer Answer](offer-answer): Example offer-answer is an example of two webrtc-rs or pion instances communicating directly!
- [x] [] [ICE Restart](ice-restart): The ice-restart demonstrates webrtc-rs ICE Restart abilities.
