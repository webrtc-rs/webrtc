# play-from-disk-hevc

play-from-disk-hevc demonstrates how to send hevc video and/or audio to your browser from files saved to disk.

## Instructions

### Create IVF named `output.265` that contains a hevc track and/or `output.ogg` that contains a Opus track

```shell
ffmpeg -i $INPUT_FILE -an -c:v libx265 -bsf:v hevc_mp4toannexb -b:v 2M -max_delay 0 -bf 0 output.265
ffmpeg -i $INPUT_FILE -c:a libopus -page_duration 20000 -vn output.ogg
```

### Build/Run play-from-disk-hevc

```shell
cargo run --example play-from-disk-hevc
```

### Result and Output
In the shell you opened, you should see from std that rtp of hevc get received and parsed 

After all is done, an `xx.output` file should be created at the same directory of the src video file

Congrats, you have sent and received the hevc stream

## Notes  
- Maybe you will need to install libx265/opus for your ffmepg
- Please update the stun server to the best match, google maybe slow/unaccessable in some certain region/circumstance
