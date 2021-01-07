use webrtc_rs_mdns as mdns;

use mdns::{config::*, conn::*};

use tokio::net::UdpSocket;

use util::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::Builder::new().init();

    let socket = UdpSocket::bind("0.0.0.0:8888").await?;
    println!("socket.local_addr={:?}", socket.local_addr());

    let _server = DNSConn::server(
        socket,
        Config {
            dst_port: Some(9999),
            local_names: vec![
                "webrtc-rs-mdns-1.local".to_owned(),
                "webrtc-rs-mdns-2.local".to_owned(),
            ],
            ..Default::default()
        },
    )?;

    loop {}
}
