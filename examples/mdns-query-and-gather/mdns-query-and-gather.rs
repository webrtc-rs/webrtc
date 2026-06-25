//! Async mDNS query-and-gather example.
//!
//! This mirrors the sans-I/O `rtc` example, but uses the async `webrtc` API.
//! It accepts a remote offer over stdin or a file, creates an answer with mDNS
//! enabled, prints the base64-encoded answer, and echoes any received data
//! channel messages back to the sender.

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::peer_connection::transport::RTCDtlsRole;
use signal::get_local_ip;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCIceCandidateType, RTCIceConnectionState, RTCIceGatheringState,
    RTCIceServer, RTCPeerConnectionState, Registry, SettingEngine, register_default_interceptors,
};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime};

use rtc::ice::mdns::MulticastDnsMode;

#[derive(Parser)]
#[command(name = "mdns-query-and-gather")]
#[command(author = "Rusty Rain <y@liu.mx>")]
#[command(version = "0.0.0")]
#[command(about = "An async WebRTC example of mDNS query and gather")]
struct Cli {
    #[arg(short, long)]
    client: bool,
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    input_sdp_file: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    #[arg(long, default_value_t = format!(""))]
    host: String,
    #[arg(long, default_value_t = 0)]
    port: u16,
    #[arg(short, long)]
    query_only: bool,
}

#[derive(Clone)]
struct MdnsHandler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for MdnsHandler {
    async fn on_ice_candidate(&self, event: webrtc::peer_connection::RTCPeerConnectionIceEvent) {
        if event.candidate.typ == RTCIceCandidateType::Host {
            println!("Gathered ICE host candidate: {}", event.candidate.address);
        }
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        println!("ICE Gathering State has changed: {state}");
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_ice_connection_state_change(&self, state: RTCIceConnectionState) {
        println!("ICE Connection State has changed: {state}");
        if state == RTCIceConnectionState::Failed {
            eprintln!("ICE Connection State has gone to failed! Exiting...");
            let _ = self.done_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer Connection State has changed: {state}");
        if state == RTCPeerConnectionState::Failed {
            eprintln!("Peer Connection State has gone to failed! Exiting...");
            let _ = self.done_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let done_tx = self.done_tx.clone();
        let runtime = self.runtime.clone();
        runtime.spawn(Box::pin(async move {
            let label = dc.label().await.unwrap_or_default();
            let id = dc.id();
            loop {
                match dc.poll().await {
                    Some(DataChannelEvent::OnOpen) => {
                        println!("Data channel '{label}'-'{id}' open");
                    }
                    Some(DataChannelEvent::OnMessage(msg)) => {
                        let text = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                        println!("Message from DataChannel '{label}': '{text}', Echoing back");
                        if let Err(err) = dc.send_text(&text).await {
                            eprintln!("failed to echo data channel message: {err}");
                            let _ = done_tx.try_send(());
                            break;
                        }
                    }
                    Some(DataChannelEvent::OnClose) | None => {
                        let _ = done_tx.try_send(());
                        break;
                    }
                    _ => {}
                }
            }
        }));
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let log_level = log::LevelFilter::from_str(&cli.log_level)?;
    if cli.debug {
        env_logger::Builder::new()
            .target(if !cli.output_log_file.is_empty() {
                Target::Pipe(Box::new(
                    OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(&cli.output_log_file)?,
                ))
            } else {
                Target::Stdout
            })
            .format(|buf, record| {
                writeln!(
                    buf,
                    "{}:{} [{}] {} - {}",
                    record.file().unwrap_or("unknown"),
                    record.line().unwrap_or(0),
                    record.level(),
                    chrono::Local::now().format("%H:%M:%S.%6f"),
                    record.args()
                )
            })
            .filter(None, log_level)
            .init();
    }

    block_on(async_main(cli))
}

async fn async_main(cli: Cli) -> Result<()> {
    let host = if cli.host.is_empty() {
        get_local_ip().to_string()
    } else {
        cli.host
    };
    let local_ip = IpAddr::from_str(&host)?;

    let runtime = default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;

    let (done_tx, mut done_rx) = channel::<()>(1);
    let (gather_tx, mut gather_rx) = channel::<()>(1);
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    let mut media = MediaEngine::default();
    media.register_default_codecs()?;
    let registry = register_default_interceptors(Registry::new(), &mut media)?;

    let mut setting_engine = SettingEngine::default();
    setting_engine.set_answering_dtls_role(if cli.client {
        RTCDtlsRole::Client
    } else {
        RTCDtlsRole::Server
    })?;
    setting_engine.set_multicast_dns_timeout(Some(Duration::from_secs(10)));
    if cli.query_only {
        setting_engine.set_multicast_dns_mode(MulticastDnsMode::QueryOnly);
    } else {
        setting_engine.set_multicast_dns_mode(MulticastDnsMode::QueryAndGather);
        setting_engine
            .set_multicast_dns_local_name("webrtc-rs-hides-local-ip-by-mdns.local".to_string());
        setting_engine.set_multicast_dns_local_ip(Some(local_ip));
    }

    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(
            RTCConfigurationBuilder::new()
                .with_ice_servers(vec![RTCIceServer {
                    urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                    ..Default::default()
                }])
                .build(),
        )
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .with_setting_engine(setting_engine)
        .with_handler(Arc::new(MdnsHandler {
            runtime: runtime.clone(),
            gather_complete_tx: gather_tx,
            done_tx: done_tx.clone(),
        }))
        .with_runtime(runtime)
        .with_udp_addrs(vec![format!("{host}:{}", cli.port)])
        .build()
        .await?;

    let line = if cli.input_sdp_file.is_empty() {
        signal::must_read_stdin()?
    } else {
        fs::read_to_string(&cli.input_sdp_file)?
    };
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str(&desc_data)?;
    println!("Offer received: {}", offer);

    peer_connection.set_remote_description(offer).await?;
    let answer = peer_connection.create_answer(None).await?;
    peer_connection.set_local_description(answer).await?;
    let _ = gather_rx.recv().await;

    if let Some(local_desc) = peer_connection.local_description().await {
        println!("answer created: {}", local_desc);
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = signal::encode(&json_str);
        println!("{b64}");
    } else {
        return Err(anyhow::anyhow!("generate local_description failed"));
    }

    println!("Press Ctrl-C to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => {}
        _ = ctrlc_rx.recv().fuse() => {}
    }

    peer_connection.close().await?;
    Ok(())
}
