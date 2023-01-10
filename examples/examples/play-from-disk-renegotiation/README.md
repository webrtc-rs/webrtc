# play-from-disk-renegotiation

play-from-disk-renegotiation demonstrates WebRTC.rs's renegotiation abilities.

For a simpler example of playing a file from disk we also have [examples/play-from-disk](/examples/play-from-disk)

## Instructions

### Build play-from-disk-renegotiation

```shell
cargo build --example play-from-disk-renegotiation
```

### Create IVF named `output.ivf` that contains a VP8 track

```shell
ffmpeg -i $INPUT_FILE -g 30 output.ivf
```

### Run play-from-disk-renegotiation

The `output.ivf` you created should be in the same directory as `play-from-disk-renegotiation`.


### Open the Web UI

Open [http://localhost:8080](http://localhost:8080) and you should have a `Add Track` and `Remove Track` button.  Press these to add as many tracks as you want, or to remove as many as you wish.

Congrats, you have used WebRTC.rs!
