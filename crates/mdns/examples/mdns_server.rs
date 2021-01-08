use mdns::{config::*, conn::*};
use signal_hook::{consts::SIGINT, iterator::Signals};
use tokio::net::UdpSocket;
use webrtc_rs_mdns as mdns;

#[tokio::main]
async fn main() {
    env_logger::Builder::new().init();

    let mut signals = Signals::new(&[SIGINT]).unwrap();

    let socket = UdpSocket::bind(("0.0.0.0", 5333)).await.unwrap();
    //  socket.connect(mdns::DEFAULT_ADDRESS).await.unwrap();

    let _server = DNSConn::server(
        socket,
        Config {
            local_names: vec!["webrtc-rs-mdns-2.local".to_owned()],
            ..Default::default()
        },
    )
    .unwrap();

    for _ in signals.forever() {
        log::info!("Received ctrl-c signal. Exiting.");
        return;
    }
}
