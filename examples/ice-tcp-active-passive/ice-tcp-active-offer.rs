//! ice-tcp-active-offer demonstrates the offering side with TCP active candidates.
//!
//! This example shows:
//! - TCP active candidate creation (initiates outgoing TCP connections)
//! - Signaling via HTTP with the answer side

use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use futures::FutureExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode};
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::MediaEngine;
use rtc::peer_connection::state::RTCPeerConnectionState;
use webrtc::data_channel::DataChannelEvent;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCPeerConnectionIceEvent,
    Registry, SettingEngine,
};
use webrtc::runtime::{block_on, channel, default_runtime, sleep};

#[derive(Parser)]
#[command(name = "ice-tcp-active-offer")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "ICE TCP active offerer - initiates TCP connections", long_about = None)]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(long, default_value = "0.0.0.0:50000")]
    http_address: String,
    #[arg(long, default_value = "localhost:60000")]
    answer_address: String,
}

struct OfferHandler {
    answer_address: String,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for OfferHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if let Ok(cand_init) = event.candidate.to_json() {
            if !cand_init.candidate.is_empty() {
                println!(
                    "[Offer] Local ICE candidate gathered: {}",
                    cand_init.candidate
                );
                let answer_addr = self.answer_address.clone();
                let payload = cand_init.candidate.clone();
                tokio::spawn(async move {
                    let req = Request::builder()
                        .method(Method::POST)
                        .uri(format!("http://{answer_addr}/candidate"))
                        .header("content-type", "text/plain")
                        .body(Body::from(payload));
                    if let Ok(req) = req {
                        let _ = Client::new().request(req).await;
                    }
                });
            }
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("[Offer] Peer Connection State: {state}");
    }
}

async fn remote_handler(
    req: Request<Body>,
    pc: Arc<dyn PeerConnection>,
) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/candidate") => {
            let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
            let candidate_str = match std::str::from_utf8(&body_bytes) {
                Ok(s) => s.to_owned(),
                Err(_) => {
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from("invalid utf-8"))
                        .unwrap());
                }
            };

            println!("[Offer] Received remote candidate: {}", candidate_str);
            let candidate_init = webrtc::peer_connection::RTCIceCandidateInit {
                candidate: candidate_str,
                sdp_mid: Some("".to_owned()),
                sdp_mline_index: Some(0),
                ..Default::default()
            };

            if let Err(e) = pc.add_ice_candidate(candidate_init).await {
                eprintln!("[Offer] Failed to add remote candidate: {e}");
            }

            Ok(Response::new(Body::empty()))
        }

        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("not found"))
            .unwrap()),
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
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    let runtime = default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;

    let mut media = MediaEngine::default();
    media.register_default_codecs()?;
    let registry = register_default_interceptors(Registry::new(), &mut media)?;

    let mut setting_engine = SettingEngine::default();
    setting_engine.set_network_types(vec![
        rtc::ice::network_type::NetworkType::Tcp4,
        rtc::ice::network_type::NetworkType::Tcp6,
    ]);

    let pc = PeerConnectionBuilder::new()
        .with_configuration(RTCConfigurationBuilder::new().build())
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .with_setting_engine(setting_engine)
        .with_handler(Arc::new(OfferHandler {
            answer_address: cli.answer_address.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_tcp_addrs(vec!["127.0.0.1:0".to_owned()]) // Binds to ephemeral port for TCP active candidate
        .with_udp_addrs(Vec::<String>::new())
        .build()
        .await?;

    let pc_arc = Arc::new(pc);

    // Create the data channel
    let dc = pc_arc.create_data_channel("data", None).await?;

    let label = dc.label().await.unwrap_or_default();
    let id = dc.id();

    runtime.spawn(Box::pin(async move {
        loop {
            match dc.poll().await {
                Some(DataChannelEvent::OnOpen) => {
                    println!("[Offer] Data channel '{label}'-'{id}' is open!");
                    break;
                }
                Some(DataChannelEvent::OnClose) | None => {
                    println!("[Offer] Data channel '{label}'-'{id}' closed before opening.");
                    return;
                }
                _ => {}
            }
        }

        // Periodically send message every 3 seconds
        let mut send_timer = Box::pin(sleep(Duration::from_secs(3)));
        loop {
            futures::select! {
                event = dc.poll().fuse() => {
                    match event {
                        Some(DataChannelEvent::OnMessage(msg)) => {
                            let text = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                            println!("[Offer] Message from DataChannel '{label}': '{text}'");
                        }
                        Some(DataChannelEvent::OnClose) | None => {
                            println!("[Offer] Data channel '{label}'-'{id}' closed.");
                            break;
                        }
                        _ => {}
                    }
                }
                _ = send_timer.as_mut().fuse() => {
                    let message = format!("[Offer] {}", chrono::Local::now());
                    println!("[Offer] Sending: '{message}'");
                    let _ = dc.send_text(&message).await;
                    send_timer = Box::pin(sleep(Duration::from_secs(3)));
                }
            }
        }
    }));

    // Start HTTP server to receive remote candidates
    let addr: SocketAddr = cli.http_address.parse()?;
    let pc_srv = pc_arc.clone();
    runtime.spawn(Box::pin(async move {
        let make_svc = make_service_fn(move |_| {
            let pc = pc_srv.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |req| remote_handler(req, pc.clone())))
            }
        });
        if let Err(e) = Server::bind(&addr).serve(make_svc).await {
            eprintln!("HTTP server error: {e}");
        }
    }));

    // Give HTTP server a moment to start
    sleep(Duration::from_millis(100)).await;

    // Create offer
    let offer = pc_arc.create_offer(None).await?;
    pc_arc.set_local_description(offer).await?;

    let offer_sdp = pc_arc
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("no local description"))?;

    println!(
        "[Offer] Sending offer to answer server at http://{}/sdp",
        cli.answer_address
    );

    // POST the offer SDP to the answerer and receive the answer in the response
    let payload = serde_json::to_string(&offer_sdp)?;
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("http://{}/sdp", cli.answer_address))
        .header("content-type", "application/json; charset=utf-8")
        .body(Body::from(payload))?;

    let resp = Client::new().request(req).await?;
    let body_bytes = hyper::body::to_bytes(resp.into_body()).await?;
    let answer_sdp = serde_json::from_slice(&body_bytes)?;

    println!("[Offer] Received answer from remote peer");
    pc_arc.set_remote_description(answer_sdp).await?;

    println!(
        "[Offer] HTTP server listening on http://{}",
        cli.http_address
    );
    println!("[Offer] Press ctrl-c to stop");
    ctrlc_rx.recv().await;
    println!();

    pc_arc.close().await?;
    Ok(())
}
