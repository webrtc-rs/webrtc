//! udp-mux serves any number of browser data-channel sessions over **one** UDP port.
//!
//! This example demonstrates:
//! - `UDPMuxDefault` sharing one UDP socket across every peer connection
//! - HTTP signaling (offer → answer, trickle ICE), one browser tab per connection
//! - Why: with per-connection binds (`with_udp_addrs`), pinning one port fails the second
//!   connection with `EADDRINUSE`; with a mux, only the mux binds, so one firewall/forward
//!   rule covers every session — the deployment shape of servers like SRS or mediamtx.
//!
//! Run it, open http://127.0.0.1:8080 in several browser tabs, and connect them all:
//! every session's media arrives on UDP port 9002.

use anyhow::Result;
use futures::FutureExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use log::{error, info};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::transport::udp_mux::{UDPMux, UDPMuxDefault};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{
    RTCIceCandidateInit, RTCIceGatheringState, RTCPeerConnectionState, RTCSessionDescription,
};
use webrtc::runtime::{Runtime, Sender, channel};

#[path = "../common/mod.rs"]
mod common;
use common::{block_on, runtime, timeout};

const DEMO_HTML: &str = include_str!("../data-channels-simple/demo.html");

/// The UDP port every session shares.
const MUX_UDP_PORT: u16 = 9002;

// ── Shared state for HTTP handlers ─────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    offer_tx: Sender<(
        RTCSessionDescription,
        Sender<Result<RTCSessionDescription, String>>,
    )>,
    /// Trickle candidates are handed to the most recent session, matching
    /// data-channels-simple: with the server answering (ICE-controlled), the browser's
    /// trickled candidates are redundant — the checks the browser initiates already carry
    /// everything the server side learns from them (peer-reflexive candidates).
    candidate_tx: Sender<RTCIceCandidateInit>,
}

// ── WebRTC event handler ───────────────────────────────────────────────────────

struct Handler {
    id: usize,
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
        info!("[session {}] connection state: {}", self.id, state);
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let id = self.id;
        self.runtime.spawn(Box::pin(async move {
            let label = dc.label().await.unwrap_or_default();
            info!("[session {id}] data channel '{label}' open");
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        if let Err(e) = dc.send_text("Hello from the shared UDP port!").await {
                            error!("[session {id}] failed to greet: {e}");
                        }
                    }
                    DataChannelEvent::OnMessage(msg) => {
                        info!(
                            "[session {id}] '{label}': {}",
                            String::from_utf8_lossy(&msg.data)
                        );
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
            info!("[session {id}] data channel '{label}' closed");
        }));
    }
}

// ── Entry point ────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    block_on(async_main())
}

async fn async_main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let runtime = runtime();

    // The one socket every session shares. Open or forward exactly this UDP port.
    let mux_socket = std::net::UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], MUX_UDP_PORT)))?;
    let mux = UDPMuxDefault::new(runtime.clone(), mux_socket)?;
    info!("UDP mux: all sessions share {}", mux.local_addr()?);

    let (offer_tx, mut offer_rx) = channel::<(
        RTCSessionDescription,
        Sender<Result<RTCSessionDescription, String>>,
    )>(8);
    let (candidate_tx, mut candidate_rx) = channel::<RTCIceCandidateInit>(64);

    let state = Arc::new(AppState {
        offer_tx,
        candidate_tx,
    });

    let addr: SocketAddr = "127.0.0.1:8080".parse()?;
    info!("Signaling server on http://{addr} — open it in several tabs and connect them all");

    {
        let state = state.clone();
        runtime.spawn(Box::pin(async move {
            let make_svc = make_service_fn(move |_| {
                let state = state.clone();
                async move {
                    Ok::<_, hyper::Error>(service_fn(move |req| handle_request(req, state.clone())))
                }
            });
            if let Err(e) = Server::bind(&addr).serve(make_svc).await {
                error!("HTTP server error: {e}");
            }
        }));
    }

    // Every peer connection is built on the same mux. The connections stay alive in `peers`,
    // indexed by the session id the candidates arrive under.
    let mut peers: Vec<Arc<dyn PeerConnection>> = Vec::new();

    loop {
        futures::select! {
            msg = offer_rx.recv().fuse() => {
                let Some((offer, response_tx)) = msg else { break };

                let id = peers.len();
                let (gather_tx, mut gather_rx) = channel::<()>(1);
                let handler = Arc::new(Handler {
                    id,
                    gather_complete_tx: gather_tx,
                    runtime: runtime.clone(),
                });

                let result = async {
                    let pc = PeerConnectionBuilder::<String>::new()
                        .with_handler(handler)
                        .with_runtime(runtime.clone())
                        .with_udp_mux(mux.clone())
                        .build()
                        .await?;
                    pc.set_remote_description(offer).await?;
                    let answer = pc.create_answer(None).await?;
                    pc.set_local_description(answer).await?;
                    let _ = timeout(Duration::from_secs(5), gather_rx.recv()).await;
                    let local = pc
                        .local_description()
                        .await
                        .ok_or_else(|| anyhow::anyhow!("no local description"))?;
                    Ok::<(Arc<dyn PeerConnection>, RTCSessionDescription), anyhow::Error>((Arc::new(pc), local))
                }
                .await;

                match result {
                    Ok((pc, answer)) => {
                        info!("[session {id}] answered; {} session(s) on UDP/{MUX_UDP_PORT}", peers.len() + 1);
                        peers.push(pc);
                        response_tx.try_send(Ok(answer)).ok();
                    }
                    Err(e) => {
                        error!("[session {id}] setup failed: {e}");
                        response_tx.try_send(Err(e.to_string())).ok();
                    }
                }
            }

            msg = candidate_rx.recv().fuse() => {
                let Some(candidate) = msg else { break };
                if let Some(pc) = peers.last()
                    && let Err(e) = pc.add_ice_candidate(candidate).await
                {
                    error!("failed to add ICE candidate: {e}");
                }
            }
        }
    }

    mux.close();
    Ok(())
}

// ── HTTP request handler ───────────────────────────────────────────────────────

async fn handle_request(
    req: Request<Body>,
    state: Arc<AppState>,
) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(Response::builder()
            .header("Content-Type", "text/html")
            .body(Body::from(DEMO_HTML))
            .unwrap()),

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

            let (response_tx, mut response_rx) =
                channel::<Result<RTCSessionDescription, String>>(1);
            if state.offer_tx.try_send((offer, response_tx)).is_err() {
                return Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("WebRTC loop not running"))
                    .unwrap());
            }

            match response_rx.recv().await {
                Some(Ok(answer)) => Ok(Response::builder()
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&answer).unwrap()))
                    .unwrap()),
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

        (&Method::POST, "/candidate") => {
            let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
            let body_str = String::from_utf8_lossy(&body_bytes);
            match serde_json::from_str::<RTCIceCandidateInit>(&body_str) {
                Ok(candidate) => {
                    state.candidate_tx.try_send(candidate).ok();
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .body(Body::empty())
                        .unwrap())
                }
                Err(e) => Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(e.to_string()))
                    .unwrap()),
            }
        }

        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap()),
    }
}
