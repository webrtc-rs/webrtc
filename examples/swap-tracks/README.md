# swap-tracks

swap-tracks demonstrates how to swap multiple incoming tracks on a single outgoing track.

## Instructions

### Build swap-tracks

```shell
cargo build --example swap-tracks
```

### Open swap-tracks example page

[jsfiddle.net](https://jsfiddle.net/dzc17fga/) you should see two text-areas and a 'Start Session' button.

### Run swap-tracks, with your browsers SessionDescription as stdin

In the jsfiddle the top textarea is your browser, copy that and:

#### Linux/macOS

Run `echo $BROWSER_SDP | ./target/debug/examples/swap-tracks`

#### Windows

1. Paste the SessionDescription into a file.
1. Run `./target/debug/examples/swap-tracks < my_file`

### Input swap-tracks's SessionDescription into your browser

Copy the text that `swap-tracks` just emitted and copy into second text area

### Hit 'Start Session' in jsfiddle, enjoy your video!

Your browser should send streams to webrtc-rs, and then a stream will be relayed back, changing every 5 seconds.

Congrats, you have used WebRTC.rs!
