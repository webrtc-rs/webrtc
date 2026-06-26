use anyhow::Result;
use clap::Parser;
use webrtc::peer_connection::RTCIceTransportPolicy;

#[path = "../trickle_ice_common/mod.rs"]
mod trickle_ice_common;

use trickle_ice_common::{TrickleCli, TrickleExampleConfig, init_logging, run_example};

#[derive(Parser)]
#[command(name = "trickle-ice-host")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "Async trickle ICE example with host local candidates")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
}

fn main() -> Result<()> {
    webrtc::runtime::block_on(async_main())
}

async fn async_main() -> Result<()> {
    let cli = Cli::parse();
    let shared = TrickleCli {
        debug: cli.debug,
        log_level: cli.log_level,
        output_log_file: cli.output_log_file,
    };
    init_logging(&shared)?;
    run_example(
        shared,
        TrickleExampleConfig {
            name: "trickle-ice-host",
            ice_servers: vec![],
            ice_transport_policy: RTCIceTransportPolicy::All,
        },
    )
    .await
}
