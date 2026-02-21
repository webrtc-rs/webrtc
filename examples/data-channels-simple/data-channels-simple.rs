//! data-channels-simple is a simple datachannel demo with HTTP signaling server.
//!
//! This example demonstrates:
//! - HTTP server for WebRTC signaling
//! - Browser-based DataChannel communication
//! - ICE candidate exchange via HTTP endpoints
//! - Real-time messaging between browser and Rust server

use anyhow::Result;
use futures::FutureExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use log::{error, info};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{
    RTCConfigurationBuilder, RTCIceCandidateInit, RTCIceGatheringState, RTCIceServer,
    RTCPeerConnectionState, RTCSessionDescription,
};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, timeout};

const DEMO_HTML: &str = include_str!("demo.html");

// ── Shared state for HTTP handlers ─────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    offer_tx: Sender<(
        RTCSessionDescription,
        Sender<Result<RTCSessionDescription, String>>,
    )>,
    candidate_tx: Sender<RTCIceCandidateInit>,
}

// ── WebRTC event handler ───────────────────────────────────────────────────────

struct Handler {
    gather_complete_tx: Sender<()>,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        info!("Connection state: {}", state);
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let runtime = self.runtime.clone();
        runtime.spawn(Box::pin(async move {
            let label = dc.label().await.unwrap_or_default();
            info!("New DataChannel: '{}'", label);
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        info!("DataChannel '{}' open, sending greeting", label);
                        if let Err(e) = dc.send_text("Hello from Rust server!").await {
                            error!("Failed to send greeting: {}", e);
                        }
                    }
                    DataChannelEvent::OnMessage(msg) => {
                        let text = String::from_utf8_lossy(&msg.data);
                        info!("Received from '{}': {}", label, text);
                    }
                    DataChannelEvent::OnClose => {
                        info!("DataChannel '{}' closed", label);
                        break;
                    }
                    _ => {}
                }
            }
        }));
    }
}

// ── Entry point ────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    block_on(async_main())
}

async fn async_main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let (offer_tx, mut offer_rx) = channel::<(
        RTCSessionDescription,
        Sender<Result<RTCSessionDescription, String>>,
    )>();
    let (candidate_tx, mut candidate_rx) = channel::<RTCIceCandidateInit>();

    let state = Arc::new(AppState {
        offer_tx,
        candidate_tx,
    });

    // Start HTTP signaling server in background
    let addr: SocketAddr = "127.0.0.1:8080".parse()?;
    info!("Signaling server started on http://{}", addr);

    let state_clone = state.clone();
    runtime.spawn(Box::pin(async move {
        let make_svc = make_service_fn(move |_| {
            let state = state_clone.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |req| handle_request(req, state.clone())))
            }
        });
        let server = Server::bind(&addr).serve(make_svc);
        if let Err(e) = server.await {
            error!("HTTP server error: {}", e);
        }
    }));

    // Main loop: process offers and ICE candidates forwarded from HTTP handlers
    let mut peer_connection: Option<Arc<dyn PeerConnection>> = None;

    loop {
        futures::select! {
            msg = offer_rx.recv().fuse() => {
                let Some((offer, response_tx)) = msg else { break };
                info!("Received offer from browser");

                let (gather_tx, mut gather_rx) = channel::<()>();
                let handler = Arc::new(Handler {
                    gather_complete_tx: gather_tx,
                    runtime: runtime.clone(),
                });

                let config = RTCConfigurationBuilder::new()
                    .with_ice_servers(vec![RTCIceServer {
                        urls: vec!["stun:stun.l.google.com:19302".to_string()],
                        ..Default::default()
                    }])
                    .build();

                match PeerConnectionBuilder::new()
                    .with_configuration(config)
                    .with_handler(handler)
                    .with_runtime(runtime.clone())
                    .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
                    .build()
                    .await
                {
                    Ok(pc) => {
                        if let Err(e) = pc.set_remote_description(offer).await {
                            response_tx.try_send(Err(e.to_string())).ok();
                            continue;
                        }
                        match pc.create_answer(None).await {
                            Ok(answer) => {
                                if let Err(e) = pc.set_local_description(answer).await {
                                    response_tx.try_send(Err(e.to_string())).ok();
                                    continue;
                                }
                                // Wait for ICE gathering to complete before returning the answer
                                let _ = timeout(Duration::from_secs(5), gather_rx.recv()).await;
                                match pc.local_description().await {
                                    Some(local_desc) => {
                                        response_tx.try_send(Ok(local_desc)).ok();
                                    }
                                    None => {
                                        response_tx
                                            .try_send(Err("No local description".to_string()))
                                            .ok();
                                        continue;
                                    }
                                }
                                let pc: Arc<dyn PeerConnection> = Arc::new(pc);
                                peer_connection = Some(pc);
                            }
                            Err(e) => {
                                response_tx.try_send(Err(e.to_string())).ok();
                            }
                        }
                    }
                    Err(e) => {
                        response_tx.try_send(Err(e.to_string())).ok();
                    }
                }
            }

            msg = candidate_rx.recv().fuse() => {
                let Some(candidate) = msg else { break };
                if let Some(pc) = peer_connection.as_ref() {
                    if let Err(e) = pc.add_ice_candidate(candidate).await {
                        error!("Failed to add ICE candidate: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

// ── HTTP request handler ───────────────────────────────────────────────────────

async fn handle_request(
    req: Request<Body>,
    state: Arc<AppState>,
) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        // Serve demo HTML
        (&Method::GET, "/") => Ok(Response::builder()
            .header("Content-Type", "text/html")
            .body(Body::from(DEMO_HTML))
            .unwrap()),

        // Handle SDP offer → return answer with ICE candidates
        (&Method::POST, "/offer") => {
            let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
            let body_str = String::from_utf8_lossy(&body_bytes);

            let offer: RTCSessionDescription = match serde_json::from_str(&body_str) {
                Ok(o) => o,
                Err(e) => {
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from(e.to_string()))
                        .unwrap());
                }
            };

            let (response_tx, mut response_rx) = channel::<Result<RTCSessionDescription, String>>();

            if state.offer_tx.try_send((offer, response_tx)).is_err() {
                return Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("WebRTC loop not running"))
                    .unwrap());
            }

            match response_rx.recv().await {
                Some(Ok(answer)) => {
                    let json = serde_json::to_string(&answer).unwrap();
                    Ok(Response::builder()
                        .header("Content-Type", "application/json")
                        .body(Body::from(json))
                        .unwrap())
                }
                Some(Err(e)) => Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(e))
                    .unwrap()),
                None => Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("No response from WebRTC"))
                    .unwrap()),
            }
        }

        // Handle trickle ICE candidate from browser
        (&Method::POST, "/candidate") => {
            let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
            let body_str = String::from_utf8_lossy(&body_bytes);

            let candidate: RTCIceCandidateInit = match serde_json::from_str(&body_str) {
                Ok(c) => c,
                Err(e) => {
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from(e.to_string()))
                        .unwrap());
                }
            };

            if state.candidate_tx.try_send(candidate).is_err() {
                return Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("WebRTC loop not running"))
                    .unwrap());
            }

            Ok(Response::builder()
                .status(StatusCode::OK)
                .body(Body::empty())
                .unwrap())
        }

        // 404 for other routes
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap()),
    }
}
