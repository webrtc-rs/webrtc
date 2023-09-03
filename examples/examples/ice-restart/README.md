# ice-restart
ice-restart demonstrates WebRTC.rs ICE Restart abilities.

## Instructions

### Build ice-restart
```shell
cargo build --example ice-restart
```

### Run ice-restart
```shell
cargo run --example ice-restart
```

### Open the Web UI
Open [http://localhost:8080](http://localhost:8080). This will automatically start a PeerConnection. This page will now prints stats about the PeerConnection
and allow you to do an ICE Restart at anytime.

* `ICE Restart` is the button that causes a new offer to be made with `iceRestart: true`.
* `ICE Connection States` will contain all the connection states the PeerConnection moves through.
* `ICE Selected Pairs` will print the selected pair every 3 seconds. Note how the uFrag/uPwd/Port change everytime you start the Restart process.
* `Inbound DataChannel Messages` containing the current time sent by the Pion process every 3 seconds.

Congrats, you have used WebRTC.rs!
