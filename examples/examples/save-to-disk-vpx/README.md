# save-to-disk-vpx

save-to-disk-vpx is a simple application that shows how to record your webcam/microphone using WebRTC.rs and save VP8/VP9 and Opus to disk.

## Instructions

### Build save-to-disk-vpx

```shell
cargo build --example save-to-disk-vpx
```

### Open save-to-disk-vpx example page

[jsfiddle.net](https://jsfiddle.net/vfmcg8rk/1/) you should see your Webcam, two text-areas and a 'Start Session' button

### Run save-to-disk-vpx, with your browsers SessionDescription as stdin

In the jsfiddle the top textarea is your browser, copy that and:

#### Linux/macOS

Run `echo $BROWSER_SDP | ./target/debug/examples/save-to-disk-vpx`

#### Windows

1. Paste the SessionDescription into a file.
1. Run `./target/debug/examples/save-to-disk-vpx < my_file`

### Input save-to-disk-vpx's SessionDescription into your browser

Copy the text that `save-to-disk-vpx` just emitted and copy into second text area

### Hit 'Start Session' in jsfiddle, wait, close jsfiddle, enjoy your video!

In the folder you ran `save-to-disk-vpx` you should now have a file `output_vpx.ivf` play with your video player of choice!

Congrats, you have used WebRTC.rs!
