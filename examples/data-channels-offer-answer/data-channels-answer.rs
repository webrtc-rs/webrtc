//! Answer side of the offer/answer example.
//!
//! Run `answer` first, then `offer`.  They exchange SDP over HTTP and send
//! random messages to each other via a WebRTC data channel every 5 seconds.
//!
//! Usage:
//!   cargo run --example answer -- [--answer-address 0.0.0.0:60000] [--offer-address localhost:50000] [-d]

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use bytes::BytesMut;
use clap::Parser;
use futures::FutureExt;
use hyper::{Body, Client, Method, Request};
use signal::get_local_ip;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    MediaEngine, RTCConfigurationBuilder, RTCIceGatheringState, RTCIceServer,
    RTCPeerConnectionState, Registry, register_default_interceptors,
};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep};

#[derive(Parser)]
#[command(name = "answer", about = "WebRTC answer side")]
struct Cli {
    #[arg(long, default_value = "0.0.0.0:60000")]
    answer_address: String,
    #[arg(long, default_value = "localhost:50000")]
    offer_address: String,
    #[arg(short, long)]
    debug: bool,
}

#[derive(Clone)]
struct AnswerHandler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for AnswerHandler {
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

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let done_tx = self.done_tx.clone();
        // Must spawn: blocking here would stall the driver.
        self.runtime.spawn(Box::pin(async move {
            let mut opened = false;
            let label = dc.label().await.unwrap_or_default();
            let id = dc.id();
            let mut count = 0u32;
            let mut send_timer = Box::pin(sleep(Duration::from_secs(5)));

            loop {
                if opened {
                    futures::select! {
                        event = dc.poll().fuse() => {
                            match event {
                                Some(DataChannelEvent::OnMessage(msg)) => {
                                    let text = String::from_utf8(msg.data.to_vec())
                                        .unwrap_or_default();
                                    println!("Message from DataChannel '{label}': '{text}'");
                                }
                                Some(DataChannelEvent::OnClose) | None => {
                                    let _ = done_tx.try_send(());
                                    break;
                                }
                                _ => {}
                            }
                        }
                        _ = send_timer.as_mut().fuse() => {
                            let message = format!("answer-{count}");
                            count += 1;
                            println!("Sending '{message}'");
                            let _ = dc.send(BytesMut::from(message.as_bytes())).await;
                            send_timer = Box::pin(sleep(Duration::from_secs(5)));
                        }
                    }
                } else {
                    match dc.poll().await {
                        Some(DataChannelEvent::OnOpen) => {
                            println!("Data channel '{label}'-'{id}' open");
                            opened = true;
                            send_timer = Box::pin(sleep(Duration::from_secs(5)));
                        }
                        Some(DataChannelEvent::OnClose) | None => {
                            let _ = done_tx.try_send(());
                            break;
                        }
                        _ => {}
                    }
                }
            }

            println!("exit loop for DataChannel '{label}'-'{id}'");
        }));
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

    let pc = PeerConnectionBuilder::new()
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
        .with_handler(Arc::new(AnswerHandler {
            runtime: runtime.clone(),
            gather_complete_tx: gather_tx,
            done_tx: done_tx.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec![format!("{}:0", get_local_ip())])
        .build()
        .await?;

    // Start HTTP server to receive the offer SDP
    let answer_port = cli
        .answer_address
        .split(':')
        .last()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(60000);
    let mut sdp_rx = signal::http_sdp_server(answer_port).await;
    println!("Listening on http://{}", cli.answer_address);
    println!("Waiting for offer...");

    // Wait for offer SDP
    let offer_str = sdp_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("offer channel closed"))?;
    let offer_sdp = serde_json::from_str(&offer_str)?;

    // Complete the exchange
    pc.set_remote_description(offer_sdp).await?;
    let answer = pc.create_answer(None).await?;
    pc.set_local_description(answer).await?;

    // Wait for ICE gathering to complete (non-trickle)
    gather_rx.recv().await;

    let answer_sdp = pc
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("no local description"))?;

    // POST the answer SDP back to the offer side
    let payload = serde_json::to_string(&answer_sdp)?;
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("http://{}/sdp", cli.offer_address))
        .header("content-type", "application/json; charset=utf-8")
        .body(Body::from(payload))?;
    Client::new().request(req).await?;

    println!("Press ctrl-c to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => {
            println!("Peer connection failed or data channel closed.");
        }
        _ = ctrlc_rx.recv().fuse() => {
            println!();
        }
    }

    pc.close().await?;
    Ok(())
}
