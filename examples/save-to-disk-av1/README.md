# save-to-disk-av1

`save-to-disk-av1` records incoming AV1 video from a browser and saves it to an IVF file.

## Instructions

### Build save-to-disk-av1

```shell
cargo build --example save-to-disk-av1
```

### Open save-to-disk-av1 example page

[jsfiddle.net](https://jsfiddle.net/8jv91r25/) you should see your webcam, two text-areas and two buttons:
`Copy browser SDP to clipboard`, `Start Session`.

### Run save-to-disk-av1 with your browser's SessionDescription as stdin

In the jsfiddle the top textarea is your browser's Session Description. Press `Copy browser SDP to clipboard` or copy the
base64 string manually.

#### Linux/macOS

Run `echo $BROWSER_SDP | ./target/debug/examples/save-to-disk-av1`

#### Windows

1. Paste the SessionDescription into a file.
1. Run `./target/debug/examples/save-to-disk-av1 < my_file`

### Input save-to-disk-av1's SessionDescription into your browser

Copy the text that `save-to-disk-av1` emits and paste it into the second text area.

### Hit `Start Session`, wait, close the page, and inspect `output.ivf`

The example writes AV1 RTP into an IVF container and saves it in the file given by `--video` (default: `output.ivf`).

## Notes

- `--client` keeps the sansio example's DTLS answering-role override.
- `--host` and `--port` control the UDP bind address used by the async peer connection driver.
