# save-to-disk
save-to-disk is a simple application that shows how to record your webcam/microphone using WebRTC.rs and save VP8/VP9 and Opus to disk.

TODO: If you wish to save H264 to disk checkout out [save-to-webm]

## Instructions
### Build save-to-disk
```
cargo build --example save-to-disk
```

### Open save-to-disk example page
[jsfiddle.net](https://jsfiddle.net/vfmcg8rk/1/) you should see your Webcam, two text-areas and a 'Start Session' button

### Run save-to-disk, with your browsers SessionDescription as stdin
In the jsfiddle the top textarea is your browser, copy that and:
#### Linux/macOS
Run `echo $BROWSER_SDP | ./target/debug/examples/save-to-disk`
#### Windows
1. Paste the SessionDescription into a file.
1. Run `./target/debug/examples/save-to-disk < my_file`

### Input save-to-disk's SessionDescription into your browser
Copy the text that `save-to-disk` just emitted and copy into second text area

### Hit 'Start Session' in jsfiddle, wait, close jsfiddle, enjoy your video!
In the folder you ran `save-to-disk` you should now have a file `output.ivf` play with your video player of choice!

Congrats, you have used WebRTC.rs!
