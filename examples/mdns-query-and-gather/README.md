# mdns-query-and-gather

`mdns-query-and-gather` is an async `webrtc` example that shows how to hide a
local host ICE candidate behind an mDNS name.

## Instructions

### Open the mdns-query-and-gather example page

[jsfiddle.net](https://jsfiddle.net/e41tgovp/)

### Run mdns-query-and-gather with your browser's SessionDescription as stdin

In the jsfiddle the top textarea is your browser's session description, copy that and:

#### Linux/macOS

Run `echo $BROWSER_SDP | cargo run --example mdns-query-and-gather`

#### Windows

1. Paste the SessionDescription into a file.
1. Run `cargo run --example mdns-query-and-gather < my_file`

### Input mdns-query-and-gather's SessionDescription into your browser

Copy the text that `mdns-query-and-gather` just emitted and copy it into the second text area.

### Hit `Start Session` in jsfiddle

Under Start Session you should see `Checking` as it starts connecting. If everything worked you should see
`Data channel 'foo'-'1' open`.

Now you can put whatever you want in the `Message` textarea, and when you hit `Send Message` it should appear in your terminal and echo back.
