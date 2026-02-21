//! Offer side of the offer/answer example.
//!
//! Run `answer` first, then `offer`.  They exchange SDP over HTTP and send
//! random messages to each other via a WebRTC data channel every 5 seconds.
//!
//! Usage:
//!   cargo run --example offer -- [--offer-address 0.0.0.0:50000] [--answer-address localhost:60000] [-d]

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use bytes::BytesMut;
use clap::Parser;
use futures::FutureExt;
use hyper::{Body, Client, Method, Request};
use signal::get_local_ip;
use webrtc::data_channel::DataChannelEvent;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Sender, block_on, channel, default_runtime, sleep};
use webrtc::{
    MediaEngine, RTCConfigurationBuilder, RTCIceGatheringState, RTCIceServer,
    RTCPeerConnectionState, Registry, register_default_interceptors,
};

#[derive(Parser)]
#[command(name = "offer", about = "WebRTC offer side")]
struct Cli {
    #[arg(long, default_value = "0.0.0.0:50000")]
    offer_address: String,
    #[arg(long, default_value = "localhost:60000")]
    answer_address: String,
    #[arg(short, long)]
    debug: bool,
}

#[derive(Clone)]
struct OfferHandler {
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for OfferHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer connection state: {state}");
        if state == RTCPeerConnectionState::Failed {
            let _ = self.done_tx.try_send(());
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.debug {
        env_logger::Builder::new()
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
            .filter(None, log::LevelFilter::Trace)
            .init();
    }
    block_on(async_main(cli))
}

async fn async_main(cli: Cli) -> Result<()> {
    let (done_tx, mut done_rx) = channel::<()>();
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>();
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    let runtime = default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;

    let (gather_tx, mut gather_rx) = channel::<()>();

    let mut media = MediaEngine::default();
    media.register_default_codecs()?;
    let registry = register_default_interceptors(Registry::new(), &mut media)?;

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
        .with_handler(Arc::new(OfferHandler {
            gather_complete_tx: gather_tx,
            done_tx: done_tx.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec![format!("{}:0", get_local_ip())])
        .build()
        .await?;

    // Create the data channel
    let data_channel = peer_connection.create_data_channel("data", None).await?;

    // Event loop for the data channel: wait for open, then send every 5 s and print received messages
    runtime.spawn(Box::pin(async move {
        let mut opened = false;
        let label = match data_channel.label().await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to get data channel label: {e}");
                return;
            }
        };
        let id = data_channel.id();
        let mut count = 0u32;
        let mut send_timer = Box::pin(sleep(Duration::from_secs(5)));

        loop {
            if opened {
                futures::select! {
                    event = data_channel.poll().fuse() => {
                        match event {
                            Some(DataChannelEvent::OnMessage(msg)) => {
                                let text = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                                println!("Message from DataChannel '{label}': '{text}'");
                            }
                            Some(DataChannelEvent::OnClose) | None => break,
                            _ => {}
                        }
                    }
                    _ = send_timer.as_mut().fuse() => {
                        let message = format!("offer-{count}");
                        count += 1;
                        println!("Sending '{message}'");
                        let _ = data_channel.send(BytesMut::from(message.as_bytes())).await;
                        send_timer = Box::pin(sleep(Duration::from_secs(5)));
                    }
                }
            } else {
                match data_channel.poll().await {
                    Some(DataChannelEvent::OnOpen) => {
                        println!("Data channel '{label}'-'{id}' open");
                        opened = true;
                        send_timer = Box::pin(sleep(Duration::from_secs(5)));
                    }
                    Some(DataChannelEvent::OnClose) | None => break,
                    _ => {}
                }
            }
        }

        println!("exit loop for DataChannel '{label}'-'{id}'");
    }));

    // Create offer and wait for ICE gathering to complete (non-trickle)
    let offer = peer_connection.create_offer(None).await?;

    // Sets the LocalDescription, and starts our UDP listeners
    peer_connection.set_local_description(offer).await?;

    // Block until ICE Gathering is complete, disabling trickle ICE
    // we do this because we only can exchange one signaling message
    // in a production application you should exchange ICE Candidates via OnICECandidate
    let _ = gather_rx.recv().await;

    let offer_sdp = peer_connection
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("no local description"))?;

    // Start our HTTP server to receive the answer SDP
    let offer_port = cli
        .offer_address
        .split(':')
        .last()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(50000);
    let mut sdp_rx = signal::http_sdp_server(offer_port).await;
    println!("Listening on http://{}", cli.offer_address);

    // POST the offer SDP to the answer side
    let payload = serde_json::to_string(&offer_sdp)?;
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("http://{}/sdp", cli.answer_address))
        .header("content-type", "application/json; charset=utf-8")
        .body(Body::from(payload))?;
    Client::new().request(req).await?;

    // Wait for the answer SDP
    let answer_str = sdp_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("answer channel closed"))?;
    let answer_sdp = serde_json::from_str(&answer_str)?;
    peer_connection.set_remote_description(answer_sdp).await?;

    println!("Press ctrl-c to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => {
            println!("received done signal!");
        }
        _ = ctrlc_rx.recv().fuse() => {
            println!("received ctrl-c signal!");
        }
    }

    peer_connection.close().await?;

    Ok(())
}
