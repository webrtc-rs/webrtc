# play-from-disk-h264

play-from-disk-h264 demonstrates how to send h264 video and/or audio to your browser from files saved to disk.

## Instructions

### Create IVF named `output.264` that contains a H264 track and/or `output.ogg` that contains a Opus track

```shell
ffmpeg -i $INPUT_FILE -an -c:v libx264 -bsf:v h264_mp4toannexb -b:v 2M -max_delay 0 -bf 0 output.h264
ffmpeg -i $INPUT_FILE -c:a libopus -page_duration 20000 -vn output.ogg
```

### Build play-from-disk-h264

```shell
cargo build --example play-from-disk-h264
```

### Open play-from-disk-h264 example page

[jsfiddle.net](https://jsfiddle.net/9s10amwL/) you should see two text-areas and a 'Start Session' button

### Run play-from-disk-h264 with your browsers SessionDescription as stdin

The `output.h264` you created should be in the same directory as `play-from-disk-h264`. In the jsfiddle the top textarea is your browser, copy that and:

#### Linux/macOS

Run `echo $BROWSER_SDP | ./target/debug/examples/play-from-disk-h264 -v examples/test-data/output.h264 -a examples/test-data/output.ogg`

#### Windows

1. Paste the SessionDescription into a file.
1. Run `./target/debug/examples/play-from-disk-h264 -v examples/test-data/output.h264 -a examples/test-data/output.ogg < my_file`

### Input play-from-disk-h264's SessionDescription into your browser

Copy the text that `play-from-disk-h264` just emitted and copy into second text area

### Hit 'Start Session' in jsfiddle, enjoy your video!

A video should start playing in your browser above the input boxes. `play-from-disk-h264` will exit when the file reaches the end

Congrats, you have used WebRTC.rs!
