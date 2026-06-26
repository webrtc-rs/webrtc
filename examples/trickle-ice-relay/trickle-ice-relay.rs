use anyhow::Result;
use clap::Parser;
use webrtc::peer_connection::{RTCIceServer, RTCIceTransportPolicy};

#[path = "../trickle_ice_common/mod.rs"]
mod trickle_ice_common;

use trickle_ice_common::{TrickleCli, TrickleExampleConfig, init_logging, run_example};

#[derive(Parser)]
#[command(name = "trickle-ice-relay")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "Async trickle ICE example with TURN relay local candidates")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    #[arg(long, default_value_t = format!("127.0.0.1"))]
    turn_host: String,
    #[arg(long, default_value_t = 3478)]
    turn_port: u16,
    #[arg(long, default_value_t = format!("user=pass"))]
    turn_user: String,
}

fn main() -> Result<()> {
    webrtc::runtime::block_on(async_main())
}

async fn async_main() -> Result<()> {
    let cli = Cli::parse();
    let (turn_username, turn_password) = cli.turn_user.split_once('=').ok_or_else(|| {
        anyhow::anyhow!("Invalid TURN credentials format. Use: username=password")
    })?;
    let shared = TrickleCli {
        debug: cli.debug,
        log_level: cli.log_level,
        output_log_file: cli.output_log_file,
    };
    init_logging(&shared)?;

    run_example(
        shared,
        TrickleExampleConfig {
            name: "trickle-ice-relay",
            ice_servers: vec![RTCIceServer {
                urls: vec![format!(
                    "turn:{}:{}?transport=udp",
                    cli.turn_host, cli.turn_port
                )],
                username: turn_username.to_owned(),
                credential: turn_password.to_owned(),
            }],
            ice_transport_policy: RTCIceTransportPolicy::Relay,
        },
    )
    .await
}
