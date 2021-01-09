use mdns::{config::*, conn::*};
use signal_hook::iterator::Signals;
use webrtc_rs_mdns as mdns;

#[tokio::main]
async fn main() {
    env_logger::Builder::new().init();

    let _server = DNSConn::server(
        ("0.0.0.0", 5353),
        Config {
            local_names: vec!["webrtc-rs-mdns-2.local".to_owned()],
            ..Default::default()
        },
    )
    .unwrap();

    let mut signals = Signals::new(&[
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGUSR1,
        signal_hook::consts::SIGUSR2,
    ])
    .unwrap();
    let close = signals.handle();

    for _ in signals.forever() {
        close.close();
    }
}
