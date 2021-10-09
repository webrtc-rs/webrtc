# play-from-disk-vp8
play-from-disk-vp8 demonstrates how to send vp8 video and/or audio to your browser from files saved to disk.

## Instructions
### Create IVF named `output_vp8.ivf` that contains a VP8 track and/or `output.ogg` that contains a Opus track
```
ffmpeg -i $INPUT_FILE -g 30 output_vp8.ivf
ffmpeg -i $INPUT_FILE -c:a libopus -page_duration 20000 -vn output.ogg
```

### Build play-from-disk-vp8
```
cargo build --example play-from-disk-vp8
```

### Open play-from-disk-vp8 example page
[jsfiddle.net](https://jsfiddle.net/9s10amwL/) you should see two text-areas and a 'Start Session' button

### Run play-from-disk-vp8 with your browsers SessionDescription as stdin
The `output_vp8.ivf` you created should be in the same directory as `play-from-disk-vp8`. In the jsfiddle the top textarea is your browser, copy that and:

#### Linux/macOS
Run `echo $BROWSER_SDP | ./target/debug/examples/play-from-disk-vp8 -v examples/test-data/output_vp8.ivf -a examples/test-data/output.ogg`
#### Windows
1. Paste the SessionDescription into a file.
1. Run `./target/debug/examples/play-from-disk-vp8 -v examples/test-data/output_vp8.ivf -a examples/test-data/output.ogg < my_file`

### Input play-from-disk-vp8's SessionDescription into your browser
Copy the text that `play-from-disk-vp8` just emitted and copy into second text area

### Hit 'Start Session' in jsfiddle, enjoy your video!
A video should start playing in your browser above the input boxes. `play-from-disk-vp8` will exit when the file reaches the end

Congrats, you have used WebRTC.rs!
