# data-channels-offer-answer

data-channels-offer-answer is an example of two webrtc-rs instances communicating directly!

The SDP offer and answer are exchanged automatically over HTTP.
The `data-channels-answer` side acts like a HTTP server and should therefore be ran first.

`hyper` v0.14 requires a Tokio runtime (it uses `tokio::net::TcpListener` internally via `Server::bind`). When we run
under `runtime-smol`, there's no Tokio
reactor, so `hyper` panics. Therefore, this example must be ran in default runtime-tokio.

## Instructions

First run `data-channels-answer`:

```shell
cargo run --example data-channels-answer
```

Next, run `data-channels-offer`:

```shell
cargo run --example data-channels-offer
```

You should see them connect and start to exchange messages.
