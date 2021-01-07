use webrtc_rs_mdns as mdns;

use mdns::{config::*, conn::*};

use tokio::net::UdpSocket;

use util::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::Builder::new().init();

    let socket = UdpSocket::bind("0.0.0.0:9999").await?;

    let server = DNSConn::server(
        socket,
        Config {
            dst_port: Some(8888),
            ..Default::default()
        },
    )?;

    let (answer, src) = server.query("webrtc-rs-mdns-1.local").await?;
    println!("answer = {}, src = {}", answer, src);

    Ok(())
}
