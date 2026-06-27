//! ice-tcp-passive-answer demonstrates the answering side with TCP passive candidates.
//!
//! This example shows:
//! - TCP passive candidate creation (accepts incoming TCP connections)
//! - Signaling SDP and ICE candidates via HTTP

use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use futures::FutureExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode};
use rtc::interceptor::Registry;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::MediaEngine;
use rtc::peer_connection::state::RTCPeerConnectionState;
use rtc::peer_connection::transport::RTCDtlsRole;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCPeerConnectionIceEvent,
    SettingEngine,
};
use webrtc::runtime::{block_on, channel, default_runtime, sleep};

#[derive(Parser)]
#[command(name = "ice-tcp-active-answer")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "ICE TCP passive answerer - accepts TCP connections", long_about = None)]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(long, default_value = "127.0.0.1:8443")]
    tcp_address: String,
    #[arg(long, default_value = "0.0.0.0:60000")]
    http_address: String,
    #[arg(long, default_value = "localhost:50000")]
    offer_address: String,
}

struct AnswerHandler {
    runtime: Arc<dyn webrtc::runtime::Runtime>,
    offer_address: String,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for AnswerHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if let Ok(cand_init) = event.candidate.to_json() {
            if !cand_init.candidate.is_empty() {
                println!(
                    "[Answer] Local ICE candidate gathered: {}",
                    cand_init.candidate
                );
                let offer_addr = self.offer_address.clone();
                let payload = cand_init.candidate.clone();
                tokio::spawn(async move {
                    let req = Request::builder()
                        .method(Method::POST)
                        .uri(format!("http://{offer_addr}/candidate"))
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
        println!("[Answer] Peer Connection State: {state}");
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let label = dc.label().await.unwrap_or_default();
        let id = dc.id();
        println!("[Answer] New DataChannel: '{label}'-'{id}'");

        self.runtime.spawn(Box::pin(async move {
            loop {
                match dc.poll().await {
                    Some(DataChannelEvent::OnOpen) => {
                        println!("[Answer] Data channel '{label}'-'{id}' is open!");
                        break;
                    }
                    Some(DataChannelEvent::OnClose) | None => {
                        println!("[Answer] Data channel '{label}'-'{id}' closed before opening.");
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
                                println!("[Answer] Message from DataChannel '{label}': '{text}'");
                            }
                            Some(DataChannelEvent::OnClose) | None => {
                                println!("[Answer] Data channel '{label}'-'{id}' closed.");
                                break;
                            }
                            _ => {}
                        }
                    }
                    _ = send_timer.as_mut().fuse() => {
                        let message = format!("[Answer] {}", chrono::Local::now());
                        println!("[Answer] Sending: '{message}'");
                        if dc.send_text(&message).await.is_err() {
                            break;
                        }
                        send_timer = Box::pin(sleep(Duration::from_secs(3)));
                    }
                }
            }
        }));
    }
}

async fn remote_handler(
    req: Request<Body>,
    pc: Arc<dyn PeerConnection>,
) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/sdp") => {
            let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
            let offer_str = match std::str::from_utf8(&body_bytes) {
                Ok(s) => s.to_owned(),
                Err(_) => {
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from("invalid utf-8"))
                        .unwrap());
                }
            };
            let offer = match serde_json::from_str(&offer_str) {
                Ok(sdp) => sdp,
                Err(e) => {
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from(format!("bad SDP: {e}")))
                        .unwrap());
                }
            };

            println!("[Answer] Received offer from remote peer");

            if let Err(e) = pc.set_remote_description(offer).await {
                return Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(e.to_string()))
                    .unwrap());
            }

            let answer = match pc.create_answer(None).await {
                Ok(a) => a,
                Err(e) => {
                    return Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from(e.to_string()))
                        .unwrap());
                }
            };

            if let Err(e) = pc.set_local_description(answer.clone()).await {
                return Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(e.to_string()))
                    .unwrap());
            }

            let payload = serde_json::to_string(&answer).unwrap();
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap())
        }

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

            println!("[Answer] Received remote candidate: {}", candidate_str);
            let candidate_init = webrtc::peer_connection::RTCIceCandidateInit {
                candidate: candidate_str,
                sdp_mid: Some("".to_owned()),
                sdp_mline_index: Some(0),
                ..Default::default()
            };

            if let Err(e) = pc.add_ice_candidate(candidate_init).await {
                eprintln!("[Answer] Failed to add remote candidate: {e}");
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
    setting_engine.set_answering_dtls_role(RTCDtlsRole::Client)?;
    setting_engine.set_network_types(vec![
        rtc::ice::network_type::NetworkType::Tcp4,
        rtc::ice::network_type::NetworkType::Tcp6,
    ]);

    let pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_configuration(RTCConfigurationBuilder::new().build())
            .with_media_engine(media)
            .with_interceptor_registry(registry)
            .with_setting_engine(setting_engine)
            .with_handler(Arc::new(AnswerHandler {
                runtime: runtime.clone(),
                offer_address: cli.offer_address.clone(),
            }))
            .with_runtime(runtime.clone())
            .with_tcp_addrs(vec![cli.tcp_address.clone()])
            .with_udp_addrs(Vec::<String>::new()) // Force TCP only
            .build()
            .await?,
    );

    let addr: SocketAddr = cli.http_address.parse()?;
    let pc_srv = pc.clone();

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

    println!("[Answer] TCP passive listener at {}", cli.tcp_address);
    println!(
        "[Answer] HTTP server listening on http://{}",
        cli.http_address
    );
    println!(
        "[Answer] Waiting for offer from http://{}",
        cli.offer_address
    );
    println!("[Answer] Press ctrl-c to stop");
    ctrlc_rx.recv().await;
    println!();

    pc.close().await?;
    Ok(())
}
