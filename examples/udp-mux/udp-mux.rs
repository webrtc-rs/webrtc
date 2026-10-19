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
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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

const DEMO_HTML: &str = include_str!("demo.html");

/// The UDP port every session shares.
const MUX_UDP_PORT: u16 = 9002;
const MAX_PENDING_CANDIDATES: usize = 256;
const DISCONNECTED_TIMEOUT: Duration = Duration::from_secs(60);
const PENDING_CANDIDATE_TTL: Duration = Duration::from_secs(30);
const INVALID_SESSION_ID: &str = "session_id must contain 1 to 128 bytes and no control characters";

fn valid_session_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
}

#[derive(Default)]
struct DisconnectWatchdog {
    since: Mutex<Option<Instant>>,
}

impl DisconnectWatchdog {
    fn update(&self, state: RTCPeerConnectionState, now: Instant) {
        let mut since = self.since.lock().unwrap();
        if state == RTCPeerConnectionState::Disconnected {
            since.get_or_insert(now);
        } else {
            *since = None;
        }
    }

    fn expired(&self, now: Instant) -> bool {
        self.since
            .lock()
            .unwrap()
            .is_some_and(|since| now.duration_since(since) >= DISCONNECTED_TIMEOUT)
    }
}

struct Session {
    generation: u64,
    pc: Arc<dyn PeerConnection>,
    watchdog: Arc<DisconnectWatchdog>,
}

type PendingCandidates = HashMap<String, Vec<(Instant, RTCIceCandidateInit)>>;

fn prune_pending(candidates: &mut PendingCandidates, now: Instant) {
    candidates.retain(|_, entries| {
        entries.retain(|(received, _)| now.duration_since(*received) < PENDING_CANDIDATE_TTL);
        !entries.is_empty()
    });
}

// ── Shared state for HTTP handlers ─────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    offer_tx: Sender<(
        String,
        RTCSessionDescription,
        Sender<Result<RTCSessionDescription, String>>,
    )>,
    candidate_tx: Sender<(String, RTCIceCandidateInit)>,
}

#[derive(serde::Deserialize)]
struct OfferRequest {
    session_id: String,
    offer: RTCSessionDescription,
}

#[derive(serde::Deserialize)]
struct CandidateRequest {
    session_id: String,
    candidate: RTCIceCandidateInit,
}

// ── WebRTC event handler ───────────────────────────────────────────────────────

struct Handler {
    id: String,
    generation: u64,
    stopped_tx: async_channel::Sender<(String, u64)>,
    watchdog: Arc<DisconnectWatchdog>,
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
        self.watchdog.update(state, Instant::now());
        info!("[session {}] connection state: {}", self.id, state);
        if matches!(
            state,
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
        ) {
            let _ = self.stopped_tx.try_send((self.id.clone(), self.generation));
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let id = self.id.clone();
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

    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;
    let runtime = runtime();

    // The one socket every session shares. Open or forward exactly this UDP port.
    let mux_socket = std::net::UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], MUX_UDP_PORT)))?;
    let mux = UDPMuxDefault::new(runtime.clone(), mux_socket)?;
    info!("UDP mux: all sessions share {}", mux.local_addr()?);

    let (offer_tx, mut offer_rx) = channel::<(
        String,
        RTCSessionDescription,
        Sender<Result<RTCSessionDescription, String>>,
    )>(8);
    let (candidate_tx, mut candidate_rx) = channel::<(String, RTCIceCandidateInit)>(64);
    let (stopped_tx, stopped_rx) = async_channel::unbounded::<(String, u64)>();

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

    // Every peer connection is built on the same mux. Candidates are routed by the client
    // supplied session ID, so concurrent sessions cannot steal each other's candidates.
    let mut peers: HashMap<String, Session> = HashMap::new();
    let mut pending_candidates = PendingCandidates::new();
    let mut cleanup = runtime.interval(Duration::from_secs(1));
    let mut next_generation = 0u64;

    loop {
        futures::select! {
            _ = ctrlc_rx.recv().fuse() => break,

            msg = offer_rx.recv().fuse() => {
                let Some((id, offer, response_tx)) = msg else { break };

                if peers.contains_key(&id) {
                    let _ = response_tx.try_send(Err("session ID already exists".to_owned()));
                    continue;
                }

                let generation = next_generation;
                next_generation += 1;
                let (gather_tx, mut gather_rx) = channel::<()>(1);
                let watchdog = Arc::new(DisconnectWatchdog::default());
                let handler = Arc::new(Handler {
                    id: id.clone(),
                    generation,
                    stopped_tx: stopped_tx.clone(),
                    watchdog: watchdog.clone(),
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
                    let result = async {
                        pc.set_remote_description(offer).await?;
                        let answer = pc.create_answer(None).await?;
                        pc.set_local_description(answer).await?;
                        let _ = timeout(Duration::from_secs(5), gather_rx.recv()).await;
                        pc.local_description()
                            .await
                            .ok_or_else(|| anyhow::anyhow!("no local description"))
                    }
                    .await;
                    match result {
                        Ok(answer) => Ok::<(Arc<dyn PeerConnection>, RTCSessionDescription), anyhow::Error>((Arc::new(pc), answer)),
                        Err(e) => {
                            if let Err(close_error) = pc.close().await {
                                error!("[session {id}] failed to close peer after setup error: {close_error}");
                            }
                            Err(e)
                        }
                    }
                }
                .await;

                match result {
                    Ok((pc, answer)) => {
                        info!("[session {id}] answered; {} session(s) on UDP/{MUX_UDP_PORT}", peers.len() + 1);
                        peers.insert(id.clone(), Session { generation, pc: pc.clone(), watchdog });
                        prune_pending(&mut pending_candidates, Instant::now());
                        if let Some(candidates) = pending_candidates.remove(&id) {
                            for (_, candidate) in candidates {
                                if let Err(e) = pc.add_ice_candidate(candidate).await {
                                    error!("[session {id}] failed to add pending ICE candidate: {e}");
                                }
                            }
                        }
                        response_tx.try_send(Ok(answer)).ok();
                    }
                    Err(e) => {
                        error!("[session {id}] setup failed: {e}");
                        pending_candidates.remove(&id);
                        response_tx.try_send(Err(e.to_string())).ok();
                    }
                }
            }

            msg = candidate_rx.recv().fuse() => {
                let Some((id, candidate)) = msg else { break };
                prune_pending(&mut pending_candidates, Instant::now());
                if let Some(session) = peers.get(&id) {
                    if let Err(e) = session.pc.add_ice_candidate(candidate).await {
                        error!("[session {id}] failed to add ICE candidate: {e}");
                    }
                } else if pending_candidates.values().map(Vec::len).sum::<usize>()
                    < MAX_PENDING_CANDIDATES
                {
                    pending_candidates.entry(id).or_default().push((Instant::now(), candidate));
                } else {
                    error!("dropping ICE candidate: pending candidate limit reached");
                }
            }

            _ = cleanup.tick().fuse() => {
                let now = Instant::now();
                prune_pending(&mut pending_candidates, now);
                let expired: Vec<_> = peers.iter()
                    .filter(|(_, session)| session.watchdog.expired(now))
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in expired {
                    // Closing an earlier peer may have allowed this peer to recover.
                    if !peers[&id].watchdog.expired(Instant::now()) {
                        continue;
                    }
                    let session = peers.remove(&id).unwrap();
                    pending_candidates.remove(&id);
                    if let Err(e) = session.pc.close().await {
                        error!("[session {id}] failed to close disconnected peer: {e}");
                    }
                    info!("[session {id}] removed after disconnected timeout");
                }
            }

            event = stopped_rx.recv().fuse() => {
                let Ok((id, generation)) = event else { break };
                if peers.get(&id).is_some_and(|session| session.generation == generation) {
                    let session = peers.remove(&id).unwrap();
                    pending_candidates.remove(&id);
                    if let Err(e) = session.pc.close().await {
                        error!("[session {id}] failed to close peer: {e}");
                    }
                    info!("[session {id}] removed; {} session(s) remain", peers.len());
                }
            }
        }
    }

    for (id, session) in peers {
        if let Err(e) = session.pc.close().await {
            error!("[session {id}] failed to close peer during shutdown: {e}");
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
            let request: OfferRequest = match serde_json::from_str(&body_str) {
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
            if !valid_session_id(&request.session_id) {
                return Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(INVALID_SESSION_ID))
                    .unwrap());
            }
            if state
                .offer_tx
                .try_send((request.session_id, request.offer, response_tx))
                .is_err()
            {
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
            match serde_json::from_str::<CandidateRequest>(&body_str) {
                Ok(request) if valid_session_id(&request.session_id) => {
                    if state
                        .candidate_tx
                        .try_send((request.session_id, request.candidate))
                        .is_ok()
                    {
                        Ok(Response::builder()
                            .status(StatusCode::ACCEPTED)
                            .body(Body::empty())
                            .unwrap())
                    } else {
                        Ok(Response::builder()
                            .status(StatusCode::SERVICE_UNAVAILABLE)
                            .body(Body::from("candidate queue is full"))
                            .unwrap())
                    }
                }
                Ok(_) => Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(INVALID_SESSION_ID))
                    .unwrap()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_validation() {
        assert!(valid_session_id("a"));
        assert!(valid_session_id(&"a".repeat(128)));
        assert!(!valid_session_id(""));
        assert!(!valid_session_id(&"a".repeat(129)));
        assert!(!valid_session_id("a\nb"));
    }

    #[test]
    fn disconnected_watchdog_resets_on_recovery() {
        let watchdog = DisconnectWatchdog::default();
        let start = Instant::now();
        watchdog.update(RTCPeerConnectionState::Disconnected, start);
        assert!(!watchdog.expired(start + DISCONNECTED_TIMEOUT - Duration::from_secs(1)));
        watchdog.update(
            RTCPeerConnectionState::Connected,
            start + Duration::from_secs(5),
        );
        assert!(!watchdog.expired(start + DISCONNECTED_TIMEOUT));
        watchdog.update(
            RTCPeerConnectionState::Disconnected,
            start + Duration::from_secs(10),
        );
        assert!(watchdog.expired(start + Duration::from_secs(10) + DISCONNECTED_TIMEOUT));
    }

    #[test]
    fn pending_candidate_entries_expire() {
        let mut pending = PendingCandidates::new();
        let now = Instant::now();
        let expired = now - PENDING_CANDIDATE_TTL;
        pending.insert(
            "old".into(),
            vec![(expired, RTCIceCandidateInit::default())],
        );
        pending.insert("new".into(), vec![(now, RTCIceCandidateInit::default())]);
        prune_pending(&mut pending, now);
        assert!(!pending.contains_key("old"));
        assert_eq!(pending["new"].len(), 1);
    }
}
