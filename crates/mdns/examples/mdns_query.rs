use std::net::{Ipv4Addr, SocketAddr};

use mdns::{config::*, conn::*};
use tokio::sync::mpsc;
use webrtc_rs_mdns as mdns;

use util::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::Builder::new().init();

    let mut server = DNSConn::server(
        SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
        Config {
            local_names: vec!["webrtc-rs-mdns-2.local".to_owned()],
            ..Default::default()
        },
    )
    .unwrap();

    log::info!("querying dns");

    let (_a, b) = mpsc::channel(1);

    let (answer, src) = server.query("webrtc-rs-mdns-2.local", b).await?;
    log::info!("dns queried");
    println!("answer = {}, src = {}", answer, src);

    server.close().await?;
    Ok(())
}
