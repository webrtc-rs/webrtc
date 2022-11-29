# capture-webcam-h264
capture-webcam-h264 demonstrates how to send h264 video to your browser from a webcam on your webrtc-rs server (Linux Only).

## Instructions
### Build capture-webcam-h264
```
cargo build --example capture-webcam-h264
```

### Open capture-webcam-h264 example page
[jsfiddle.net](https://jsfiddle.net/9s10amwL/) you should see two text-areas and a 'Start Session' button

### Run capture-webcam-h264 with your browsers SessionDescription as stdin
In the jsfiddle the top textarea is your browser, copy that and:

#### Linux
Run `echo $BROWSER_SDP | ./target/debug/examples/capture-webcam-h264 -- -v /dev/video0`

### MacOS & Windows
MacOS and Windows are not supported by the h264_webcam_stream crate used in this example at this time.

### Input capture-webcam-h264's SessionDescription into your browser
Copy the text that `capture-webcam-h264` just emitted and copy into second text area

### Hit 'Start Session' in jsfiddle
A video stream of your webcam should start playing in your browser above the input boxes. `capture-webcam-h264` will exit when the file reaches the end

Congrats, you have used WebRTC.rs!
