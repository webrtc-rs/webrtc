# save-to-disk-h26x

save-to-disk-h26x is a simple application that shows how to record your webcam/microphone using WebRTC.rs and save h26x
and Opus to disk.

## Instructions

### Build save-to-disk-h26x

```shell
cargo build --example save-to-disk-h26x
```

### Open save-to-disk example page

[jsfiddle.net](https://jsfiddle.net/vfmcg8rk/1/) you should see your Webcam, two text-areas and a 'Start Session' button

### Run save-to-disk-h26x, with your browsers SessionDescription as stdin

In the jsfiddle the top textarea is your browser, copy that and:

#### Linux/macOS

Run `echo $BROWSER_SDP | ./target/debug/examples/save-to-disk-h26x`

#### Windows

1. Paste the SessionDescription into a file.
1. Run `./target/debug/examples/save-to-disk-h26x < my_file`

### Input save-to-disk-h26x's SessionDescription into your browser

Copy the text that `save-to-disk-h26x` just emitted and copy into second text area

### Hit 'Start Session' in jsfiddle, wait, close jsfiddle, enjoy your video!

In the folder you ran `save-to-disk-h26x` you should now have a file `output.h26x` play with your video player of
choice!

Congrats, you have used WebRTC.rs!
