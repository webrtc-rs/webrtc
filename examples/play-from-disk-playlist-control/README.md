# play-from-disk-playlist-control

Streams Opus pages from multi or single-track Ogg containers, exposes the playlist over an SCTP DataChannel, and lets
the browser hop between tracks while showing artist/title metadata parsed from OpusTags.

`hyper` v0.14 requires a Tokio runtime. This example is intended to run with the default `runtime-tokio` feature.

## What this showcases

- Reads multi-stream Ogg containers with `OggReader` and keeps per-track playback state.
- Publishes playlist + now-playing metadata over a DataChannel.
- Browser can send `next`, `prev`, or a 1-based track number to jump around.
- Audio is sent as Opus over RTP while metadata/control ride over SCTP.

## Prepare a demo playlist

By default, the example looks for `playlist.ogg` in the working directory. You can specify a different file with
`--playlist-file`.

**Fake two-track Ogg with metadata**

```sh
ffmpeg \
  -f lavfi -t 8 -i "sine=frequency=330" \
  -f lavfi -t 8 -i "sine=frequency=660" \
  -map 0:a -map 1:a \
  -c:a libopus -page_duration 20000 \
  -metadata:s:a:0 artist="WebRTC-rs Artist" -metadata:s:a:0 title="Fake Intro" \
  -metadata:s:a:1 artist="Open-Source Friend" -metadata:s:a:1 title="Fake Outro" \
  playlist.ogg
```

## Run it

```sh
cargo run --example play-from-disk-playlist-control
```

Or with a custom address and playlist file:

```sh
cargo run --example play-from-disk-playlist-control -- --addr 127.0.0.1:8080 --playlist-file my_music.ogg
```

Then open:

```text
http://127.0.0.1:8080
```

## Command-line options

```text
-a, --addr <ADDR>                Server address [default: 127.0.0.1:8080]
-d, --debug                      Enable debug logging
-l, --log-level <LOG_LEVEL>      Log level [default: INFO]
-o, --output-log-file <FILE>     Output log to file
-p, --playlist-file <FILE>       Playlist OGG file [default: playlist.ogg]
```
