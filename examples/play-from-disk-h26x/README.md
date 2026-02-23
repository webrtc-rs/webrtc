# play-from-disk-h26x

play-from-disk-h26x demonstrates how to send h264 video and/or audio to your browser from files saved to disk.

## Instructions

### Create video file named `output.264` that contains a H264 track and/or `output.ogg` that contains a Opus track

```shell
ffmpeg -i $INPUT_FILE -an -c:v libx264 -bsf:v h264_mp4toannexb -b:v 2M -max_delay 0 -bf 0 output.h264
ffmpeg -i $INPUT_FILE -c:a libopus -page_duration 20000 -vn output.ogg
```

### Open play-from-disk-h26x example page

[jsfiddle.net](https://jsfiddle.net/9s10amwL/) you should see two text-areas and a 'Start Session' button

### Run play-from-disk-h26x with your browsers SessionDescription as stdin

The `output.h264` you created should be in the same directory as `play-from-disk-h26x`. In the jsfiddle the top textarea
is your browser, copy that and:

#### Linux/macOS

1. Run
   `echo $BROWSER_SDP | cargo run --example play-from-disk-h26x -- -v rtc/examples/examples/test-data/output.h264 -a rtc/examples/examples/test-data/output.ogg`

2. Run
   `echo $BROWSER_SDP | cargo run --example play-from-disk-h26x -- -v rtc/examples/examples/test-data/output.h265 -a rtc/examples/examples/test-data/output.ogg --hevc`

#### Windows

1. Paste the SessionDescription into a file.
2. Run
   `cargo run --example play-from-disk-h26x -v rtc/examples/examples/test-data/output.h264 -a rtc/examples/examples/test-data/output.ogg < my_file`
3. Run
   `cargo run --example play-from-disk-h26x -v rtc/examples/examples/test-data/output.h265 -a rtc/examples/examples/test-data/output.ogg --hevc < my_file`

### Input play-from-disk-h26x's SessionDescription into your browser

Copy the text that `play-from-disk-h26x` just emitted and copy into second text area

### Hit 'Start Session' in jsfiddle, enjoy your video!

A video should start playing in your browser above the input boxes. `play-from-disk-h26x` will exit when the file
reaches the end

Congrats, you have used WebRTC.rs!
