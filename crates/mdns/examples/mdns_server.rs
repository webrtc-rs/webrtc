use std::net::{Ipv4Addr, SocketAddr};

use mdns::{config::*, conn::*};
use signal_hook::iterator::Signals;
use webrtc_rs_mdns as mdns;

#[tokio::main]
async fn main() {
    env_logger::init();

    let server = DNSConn::server(
        SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
        Config {
            local_names: vec!["webrtc-rs-mdns-2.local".to_owned()],
            ..Default::default()
        },
    )
    .unwrap();

    let mut signals = Signals::new(&[signal_hook::consts::SIGINT]).unwrap();
    let close_handle = signals.handle();

    for _sig in signals.forever() {
        println!("closing connection now");
        server.close().await.unwrap();
        close_handle.close();
        return;
    }
}
