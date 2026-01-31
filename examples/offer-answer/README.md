# offer-answer

offer-answer is an example of two webrtc-rs or pion instances communicating directly!

The SDP offer and answer are exchanged automatically over HTTP.
The `answer` side acts like a HTTP server and should therefore be ran first.

## Instructions

First run `answer`:

```shell
cargo build --example answer
./target/debug/examples/answer
```

Next, run `offer`:

```shell
cargo build --example offer
./target/debug/examples/offer
```

You should see them connect and start to exchange messages.
