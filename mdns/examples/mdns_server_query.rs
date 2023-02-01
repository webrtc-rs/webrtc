use std::net::{Ipv4Addr, SocketAddr};
use tokio::sync::mpsc;
use webrtc_mdns::{config::*, conn::*};

#[tokio::main]
async fn main() {
    env_logger::init();

    log::trace!("server a created");

    let server_a = DnsConn::server(
        SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
        Config {
            local_names: vec![
                "webrtc-rs-mdns-1.local".to_owned(),
                "webrtc-rs-mdns-2.local".to_owned(),
            ],
            ..Default::default()
        },
    )
    .unwrap();

    let server_b = DnsConn::server(
        SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
        Config {
            ..Default::default()
        },
    )
    .unwrap();

    let (a, b) = mpsc::channel(1);

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
        a.send(()).await
    });

    let (answer, src) = server_b.query("webrtc-rs-mdns-1.local", b).await.unwrap();
    println!("webrtc-rs-mdns-1.local answer = {answer}, src = {src}");

    let (a, b) = mpsc::channel(1);

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
        a.send(()).await
    });

    let (answer, src) = server_b.query("webrtc-rs-mdns-2.local", b).await.unwrap();
    println!("webrtc-rs-mdns-2.local answer = {answer}, src = {src}");

    server_a.close().await.unwrap();
    server_b.close().await.unwrap();
}
