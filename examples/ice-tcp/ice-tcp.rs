//! ice-tcp demonstrates pure WebRTC ICE TCP connectivity using the async API.
//!
//! This example shows how to configure a PeerConnection with only TCP addresses,
//! gather candidates over TCP (RFC 4571), and establish a data channel.
//!
//! Key concepts:
//! - Configuring PeerConnection with pure TCP listener via `.with_tcp_addrs()`
//! - Disabling UDP gathering
//! - Customizing DTLS role via SettingEngine
//! - Serves a local web page at http://localhost:8080 to negotiate SDP and test connection

use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use futures::FutureExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use rtc::interceptor::Registry;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::MediaEngine;
use rtc::peer_connection::state::{RTCIceGatheringState, RTCPeerConnectionState};
use rtc::peer_connection::transport::RTCDtlsRole;
use signal::get_local_ip;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, SettingEngine,
};
use webrtc::runtime::{Mutex, Runtime, Sender, block_on, channel, default_runtime, sleep};

const TCP_PORT: u16 = 8443;
const HTTP_PORT: u16 = 8080;
static INDEX_HTML: &str = "examples/ice-tcp/index.html";

#[derive(Parser)]
#[command(name = "ice-tcp", about = "ICE TCP example")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
}

/// State shared between the HTTP signaling handler and the PeerConnection event handler.
struct Shared {
    /// Signaled when ICE gathering completes for the current signaling exchange.
    gather_tx: Option<Sender<()>>,
}

struct IceTcpHandler {
    runtime: Arc<dyn Runtime>,
    shared: Arc<Mutex<Shared>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for IceTcpHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        println!("ICE Gathering State: {state}");
        if state == RTCIceGatheringState::Complete {
            if let Some(tx) = self.shared.lock().await.gather_tx.take() {
                let _ = tx.try_send(());
            }
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer Connection State: {state}");
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let label = dc.label().await.unwrap_or_default();
        let id = dc.id();
        println!("New DataChannel: '{label}'-'{id}'");

        self.runtime.spawn(Box::pin(async move {
            loop {
                match dc.poll().await {
                    Some(DataChannelEvent::OnOpen) => {
                        println!("Data channel '{label}'-'{id}' is open!");
                        break;
                    }
                    Some(DataChannelEvent::OnClose) | None => {
                        println!("Data channel '{label}'-'{id}' closed before opening.");
                        return;
                    }
                    _ => {}
                }
            }

            // Periodically send timestamp every 3 seconds
            let mut send_timer = Box::pin(sleep(Duration::from_secs(3)));
            loop {
                futures::select! {
                    event = dc.poll().fuse() => {
                        match event {
                            Some(DataChannelEvent::OnMessage(msg)) => {
                                let text = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                                println!("Message from DataChannel '{label}': '{text}'");
                            }
                            Some(DataChannelEvent::OnClose) | None => {
                                println!("Data channel '{label}'-'{id}' closed.");
                                break;
                            }
                            _ => {}
                        }
                    }
                    _ = send_timer.as_mut().fuse() => {
                        let message = chrono::Local::now().to_string();
                        println!("Sending message: '{message}'");
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
    shared: Arc<Mutex<Shared>>,
) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") | (&Method::GET, "/index.html") => {
            match std::fs::read_to_string(INDEX_HTML) {
                Ok(content) => Ok(Response::new(Body::from(content))),
                Err(_) => Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("index.html not found"))
                    .unwrap()),
            }
        }

        (&Method::POST, "/doSignaling") => {
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

            // Register a gather channel for this signaling exchange
            let (gather_tx, mut gather_rx) = channel::<()>(1);
            shared.lock().await.gather_tx = Some(gather_tx);

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

            if let Err(e) = pc.set_local_description(answer).await {
                return Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(e.to_string()))
                    .unwrap());
            }

            // Block until ICE gathering is complete (non-trickle ICE)
            gather_rx.recv().await;

            let payload = match pc.local_description().await {
                Some(desc) => match serde_json::to_string(&desc) {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::from(e.to_string()))
                            .unwrap());
                    }
                },
                None => {
                    return Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("no local description"))
                        .unwrap());
                }
            };

            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap())
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

async fn async_main(_cli: Cli) -> Result<()> {
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    let runtime = default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;

    let shared = Arc::new(Mutex::new(Shared { gather_tx: None }));

    let mut media = MediaEngine::default();
    media.register_default_codecs()?;
    let registry = register_default_interceptors(Registry::new(), &mut media)?;

    let mut setting_engine = SettingEngine::default();
    setting_engine.set_answering_dtls_role(RTCDtlsRole::Client)?;

    let local_ip = get_local_ip();
    println!("Server local IP address is: {local_ip}");

    let pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_configuration(RTCConfigurationBuilder::new().build())
            .with_media_engine(media)
            .with_interceptor_registry(registry)
            .with_setting_engine(setting_engine)
            .with_handler(Arc::new(IceTcpHandler {
                runtime: runtime.clone(),
                shared: shared.clone(),
            }))
            .with_runtime(runtime.clone())
            .with_tcp_addrs(vec![format!("{local_ip}:{TCP_PORT}")])
            .with_udp_addrs(Vec::<String>::new()) // Force TCP only
            .build()
            .await?,
    );

    let addr: SocketAddr = format!("0.0.0.0:{HTTP_PORT}").parse()?;
    let pc_srv = pc.clone();
    let shared_srv = shared.clone();

    runtime.spawn(Box::pin(async move {
        let make_svc = make_service_fn(move |_| {
            let pc = pc_srv.clone();
            let shared = shared_srv.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |req| {
                    remote_handler(req, pc.clone(), shared.clone())
                }))
            }
        });
        if let Err(e) = Server::bind(&addr).serve(make_svc).await {
            eprintln!("HTTP server error: {e}");
        }
    }));

    println!("Listening for ICE TCP connections on {local_ip}:{TCP_PORT}");
    println!("Open http://localhost:{HTTP_PORT} to access this demo");
    println!("Press ctrl-c to stop");
    ctrlc_rx.recv().await;
    println!();

    pc.close().await?;
    Ok(())
}
