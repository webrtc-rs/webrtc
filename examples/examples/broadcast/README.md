# broadcast

broadcast is a WebRTC.rs application that demonstrates how to broadcast a video to many peers, while only requiring the broadcaster to upload once.

This could serve as the building block to building conferencing software, and other applications where publishers are bandwidth constrained.

## Instructions

### Build broadcast

```shell
cargo build --example broadcast
```

### Open broadcast example page

[jsfiddle.net](https://jsfiddle.net/1jc4go7v/) You should see two buttons 'Publish a Broadcast' and 'Join a Broadcast'

### Run Broadcast

#### Linux/macOS

Run `broadcast`

### Start a publisher

* Click `Publish a Broadcast`
* Copy the string in the first input labelled `Browser base64 Session Description`
* Run `curl localhost:8080/sdp -d "$BROWSER_OFFER"`. `$BROWSER_OFFER` is the value you copied in the last step.
* The `broadcast` terminal application will respond with an answer, paste this into the second input field in your browser.
* Press `Start Session`
* The connection state will be printed in the terminal and under `logs` in the browser.

### Join the broadcast

* Click `Join a Broadcast`
* Copy the string in the first input labelled `Browser base64 Session Description`
* Run `curl localhost:8080/sdp -d "$BROWSER_OFFER"`. `$BROWSER_OFFER` is the value you copied in the last step.
* The `broadcast` terminal application will respond with an answer, paste this into the second input field in your browser.
* Press `Start Session`
* The connection state will be printed in the terminal and under `logs` in the browser.

You can change the listening port using `-port 8011`

You can `Join the broadcast` as many times as you want. The `broadcast` application is relaying all traffic, so your browser only has to upload once.

Congrats, you have used WebRTC.rs!
