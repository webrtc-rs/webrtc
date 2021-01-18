use std::net::{Ipv4Addr, SocketAddr};

use mdns::{config::*, conn::*};
use tokio::sync::mpsc;
use webrtc_rs_mdns as mdns;

#[tokio::main]
async fn main() {
    env_logger::init();

    let server = DNSConn::server(
        SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
        Config {
            ..Default::default()
        },
    )
    .unwrap();

    log::info!("querying dns");

    let (_a, b) = mpsc::channel(1);

    let (answer, src) = server.query("webrtc-rs-mdns-2.local", b).await.unwrap();
    log::info!("dns queried");
    println!("answer = {}, src = {}", answer, src);

    server.close().await.unwrap();
}
