# play-from-disk-vpx

play-from-disk-vpx demonstrates how to send vp8/vp8 video and/or audio to your browser from files saved to disk.

## Instructions

### Create IVF named `output_vp8.ivf` or `output_vp9.ivf` that contains a VP8/VP9 track and/or `output.ogg` that contains a Opus track

```shell
ffmpeg -i $INPUT_FILE -g 30 output_vp8.ivf
ffmpeg -i $INPUT_FILE -g 30 -c libvpx-vp9 output_vp9.ivf
ffmpeg -i $INPUT_FILE -map 0:a -c:a dca -ac 2 -c:a libopus -page_duration 20000 -vn output.ogg
```

### Build play-from-disk-vpx

```shell
cargo build --example play-from-disk-vpx
```

### Open play-from-disk-vpx example page

[jsfiddle.net](https://jsfiddle.net/9s10amwL/) you should see two text-areas and a 'Start Session' button

### Run play-from-disk-vpx with your browsers SessionDescription as stdin

The `output_vp8.ivf`/`output_vp9.ivf` you created should be in the same directory as `play-from-disk-vpx`. In the jsfiddle the top textarea is your browser, copy that and:

#### Linux/macOS

1. Run `echo $BROWSER_SDP | ./target/debug/examples/play-from-disk-vpx -v examples/test-data/output_vp8.ivf -a examples/test-data/output.ogg`
2. Run `echo $BROWSER_SDP | ./target/debug/examples/play-from-disk-vpx -v examples/test-data/output_vp9.ivf -a examples/test-data/output.ogg --vp9`

#### Windows

1. Paste the SessionDescription into a file.
2. Run `./target/debug/examples/play-from-disk-vpx -v examples/test-data/output_vp8.ivf -a examples/test-data/output.ogg < my_file`
3. Run `./target/debug/examples/play-from-disk-vpx -v examples/test-data/output_vp9.ivf -a examples/test-data/output.ogg --vp9 < my_file`

### Input play-from-disk-vpx's SessionDescription into your browser

Copy the text that `play-from-disk-vpx` just emitted and copy into second text area

### Hit 'Start Session' in jsfiddle, enjoy your video!

A video should start playing in your browser above the input boxes. `play-from-disk-vpx` will exit when the file reaches the end

Congrats, you have used WebRTC.rs!
