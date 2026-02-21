//! ICE restart example.
//!
//! Serves a browser page at http://localhost:8080.  The page calls `/doSignaling`
//! on load and again each time the "ICE Restart" button is pressed.  The server
//! reuses the same PeerConnection, accepting each new offer (which may carry new
//! ICE credentials) and returning an updated answer.  Watch the "ICE Connection
//! States" log in the browser to see ICE restart in action.
//!
//! Usage:
//!   cargo run --example ice-restart -- [-d]
//!   Then open http://localhost:8080 in your browser.

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
use rtc::peer_connection::transport::RTCIceServer;
use signal::get_local_ip;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Mutex, Runtime, Sender, block_on, channel, default_runtime, sleep};

static INDEX_HTML: &str = "examples/ice-restart/index.html";

#[derive(Parser)]
#[command(name = "ice-restart", about = "ICE restart example")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
}

/// State shared between the HTTP signaling handler and the PeerConnection event handler.
struct Shared {
    /// Signaled when ICE gathering completes for the current signaling exchange.
    gather_tx: Option<Sender<()>>,
}

struct IceRestartHandler {
    runtime: Arc<dyn Runtime>,
    shared: Arc<Mutex<Shared>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for IceRestartHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            if let Some(tx) = self.shared.lock().await.gather_tx.take() {
                let _ = tx.try_send(());
            }
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer connection state: {state}");
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        // Must spawn: blocking here would stall the driver.
        self.runtime.spawn(Box::pin(async move {
            // Wait for the channel to open.
            loop {
                match dc.poll().await {
                    Some(DataChannelEvent::OnOpen) => break,
                    Some(DataChannelEvent::OnClose) | None => return,
                    _ => {}
                }
            }
            // Send the current time to the browser every 3 seconds until closed.
            let mut send_timer = Box::pin(sleep(Duration::from_secs(3)));
            loop {
                futures::select! {
                    event = dc.poll().fuse() => {
                        match event {
                            Some(DataChannelEvent::OnClose) | None => break,
                            _ => {}
                        }
                    }
                    _ = send_timer.as_mut().fuse() => {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default();
                        if dc.send_text(&format!("{:.3}s", now.as_secs_f64())).await.is_err() {
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

            // Register a fresh gather channel for this signaling exchange.
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

            // Block until ICE gathering is complete (non-trickle ICE).
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

    let pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
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
            .with_handler(Arc::new(IceRestartHandler {
                runtime: runtime.clone(),
                shared: shared.clone(),
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec![format!("{}:0", get_local_ip())])
            .build()
            .await?,
    );

    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
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

    println!("Open http://localhost:8080 to access this demo");
    println!("Press ctrl-c to stop");
    ctrlc_rx.recv().await;
    println!();

    pc.close().await?;
    Ok(())
}
