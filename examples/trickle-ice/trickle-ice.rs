use anyhow::Result;
use clap::Parser;
use webrtc::peer_connection::{RTCIceServer, RTCIceTransportPolicy};

#[path = "../trickle_ice_common/mod.rs"]
mod trickle_ice_common;

use trickle_ice_common::{TrickleCli, TrickleExampleConfig, init_logging, run_example};

#[derive(Parser)]
#[command(name = "trickle-ice")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "Async trickle ICE example with host, srflx, and relay local candidates")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,

    // STUN server configuration
    #[arg(long, default_value_t = format!("stun.l.google.com:19302"))]
    stun_server: String,

    // TURN server configuration (optional)
    #[arg(long, default_value_t = format!("127.0.0.1"))]
    turn_host: String,
    #[arg(long, default_value_t = 3478)]
    turn_port: u16,
    #[arg(long, default_value_t = format!("user=pass"))]
    turn_user: String,
    #[arg(long, default_value_t = format!("webrtc.rs"))]
    turn_realm: String,

    // Candidate type flags
    #[arg(long, default_value_t = false)]
    enable_host: bool,
    #[arg(long, default_value_t = false)]
    enable_srflx: bool,
    #[arg(long, default_value_t = false)]
    enable_relay: bool,
}

fn main() -> Result<()> {
    webrtc::runtime::block_on(async_main())
}

async fn async_main() -> Result<()> {
    let mut cli = Cli::parse();
    if !cli.enable_host && !cli.enable_srflx && !cli.enable_relay {
        println!("All candidate types are disabled! Let's fallback to use Host type");
        cli.enable_host = true;
    }

    let shared = TrickleCli {
        debug: cli.debug,
        log_level: cli.log_level,
        output_log_file: cli.output_log_file,
    };
    init_logging(&shared)?;

    let mut ice_servers = vec![];
    if cli.enable_srflx {
        ice_servers.push(RTCIceServer {
            urls: vec![format!("stun:{}", cli.stun_server)],
            ..Default::default()
        });
    }
    if cli.enable_relay {
        let (turn_username, turn_password) = cli.turn_user.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("Invalid TURN credentials format. Use: username=password")
        })?;
        ice_servers.push(RTCIceServer {
            urls: vec![format!(
                "turn:{}:{}?transport=udp",
                cli.turn_host, cli.turn_port
            )],
            username: turn_username.to_owned(),
            credential: turn_password.to_owned(),
        });
    }

    // Set policy: if we only enabled relay (and not host/srflx), set policy to Relay.
    // Otherwise, set it to All.
    let ice_transport_policy = if cli.enable_relay && !cli.enable_host && !cli.enable_srflx {
        RTCIceTransportPolicy::Relay
    } else {
        RTCIceTransportPolicy::All
    };

    println!("ICE Candidate Types:");
    println!(
        "  - Host: {}",
        if cli.enable_host {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!(
        "  - ServerReflexive (STUN): {}",
        if cli.enable_srflx {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!(
        "  - Relay (TURN): {}",
        if cli.enable_relay {
            "enabled"
        } else {
            "disabled"
        }
    );

    run_example(
        shared,
        TrickleExampleConfig {
            name: "trickle-ice",
            ice_servers,
            ice_transport_policy,
        },
    )
    .await
}
