use webrtc_rs_mdns as mdns;

use mdns::{config::*, conn::*};

use tokio::net::UdpSocket;

use util::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::Builder::new().init();

    let socket = UdpSocket::bind(("0.0.0.0", 5333)).await.unwrap();

    let server = DNSConn::server(
        socket,
        Config {
            query_interval: std::time::Duration::from_secs(5),
            ..Default::default()
        },
    )?;

    log::info!("querying dns");

    let (answer, src) = server.query("webrtc-rs-mdns-2.local").await?;
    log::info!("dns queried");
    println!("answer = {}, src = {}", answer, src);

    Ok(())
}
