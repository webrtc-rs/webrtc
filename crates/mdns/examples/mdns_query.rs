use webrtc_rs_mdns as mdns;

use mdns::{config::*, conn::*};

use tokio::net::UdpSocket;

use util::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::Builder::new().init();

    let server = DNSConn::server(
        ("0.0.0.0", 5353),
        Config {
            local_names: vec!["webrtc-rs-mdns-2.local".to_owned()],
            ..Default::default()
        },
    )
    .unwrap();

    log::info!("querying dns");

    let (answer, src) = server.query("webrtc-rs-mdns-2.local").await?;
    log::info!("dns queried");
    println!("answer = {}, src = {}", answer, src);

    Ok(())
}
