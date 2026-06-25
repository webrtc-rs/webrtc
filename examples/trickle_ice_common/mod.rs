use std::io::Write;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use futures::FutureExt;
use futures::{SinkExt, StreamExt};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Response, Server, StatusCode};
use signal::get_local_ip;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCIceCandidateInit, RTCIceGatheringState, RTCIceServer,
    RTCPeerConnectionIceEvent, RTCPeerConnectionState, Registry, register_default_interceptors,
};
use webrtc::runtime::{Runtime, Sender, channel, default_runtime, sleep};

const INDEX_HTML: &str = r#"<html>
<head>
  <title>trickle-ice</title>
  <style>
    #iceConnectionStates, #inboundDataChannelMessages {
      border: 1px solid #ccc;
      padding: 10px;
      height: 200px;
      overflow-y: auto;
      font-family: monospace;
      background-color: #f9f9f9;
    }
    .ice-checking { color: orange; }
    .ice-connected { color: green; }
    .ice-disconnected { color: red; }
    .ice-closed { color: gray; }
    .data-msg { margin: 2px 0; }
  </style>
</head>
<body>
<h3>Controls</h3>
<button id="startBtn">Start</button>
<button id="stopBtn">Stop</button>

<h3> ICE Connection States </h3>
<div id="iceConnectionStates"></div> <br />

<h3> Inbound DataChannel Messages </h3>
<div id="inboundDataChannelMessages"></div>
</body>

<script>
  const socket = new WebSocket(`ws://${window.location.hostname}:8081`)
  let pc = null
  let dc = null
  let offerCreated = false

  function createPeerConnection() {
    pc = new RTCPeerConnection({})

    pc.onicecandidate = e => {
      if (e.candidate && e.candidate.candidate !== "") {
        if (socket.readyState === WebSocket.OPEN) {
          socket.send(JSON.stringify(e.candidate))
        }
      }
    }

    pc.oniceconnectionstatechange = () => {
      const el = document.createElement('p')
      el.appendChild(document.createTextNode(pc.iceConnectionState))
      el.className = 'ice-' + pc.iceConnectionState.toLowerCase()
      document.getElementById('iceConnectionStates').appendChild(el);
    }

    pc.ondatachannel = event => {
      dc = event.channel
      setupDataChannel(dc)
    }
  }

  function setupDataChannel(channel) {
    channel.onopen = () => console.log("DataChannel open")
    channel.onmessage = event => {
      const el = document.createElement('p')
      el.textContent = `${new Date().toLocaleTimeString()} - ${event.data}`
      el.className = 'data-msg'
      document.getElementById('inboundDataChannelMessages').appendChild(el)
    }
    channel.onclose = () => console.log("DataChannel closed")
  }

  socket.onmessage = async e => {
    const msg = JSON.parse(e.data)
    if (msg.candidate) {
      await pc.addIceCandidate(msg)
    } else {
      await pc.setRemoteDescription(msg)
    }
  }

  document.getElementById('startBtn').onclick = () => {
    if (offerCreated) return
    if (!pc) createPeerConnection()
    dc = pc.createDataChannel('data')
    setupDataChannel(dc)
    pc.createOffer().then(offer => {
      pc.setLocalDescription(offer)
      socket.send(JSON.stringify(offer))
      offerCreated = true
    })
  }

  document.getElementById('stopBtn').onclick = () => {
    if (dc) {
      dc.close()
      dc = null
      offerCreated = false
    }
    if (pc) {
      let el = document.createElement('p')
      el.textContent = `disconnected`
      el.className = 'ice-disconnected'
      document.getElementById('iceConnectionStates').appendChild(el)

      setTimeout(() => {
        pc.close()
        el = document.createElement('p')
        el.textContent = `closed`
        el.className = 'ice-closed'
        document.getElementById('iceConnectionStates').appendChild(el)
        pc = null
      }, 50)
    }
  }
</script>
</html>
"#;

pub struct TrickleCli {
    pub debug: bool,
    pub log_level: String,
    pub output_log_file: String,
}

pub struct TrickleExampleConfig {
    pub name: &'static str,
    pub ice_servers: Vec<RTCIceServer>,
}

#[derive(Debug)]
enum SignalMessage {
    Offer(webrtc::peer_connection::RTCSessionDescription),
    IceCandidate(RTCIceCandidateInit),
}

#[derive(Clone)]
struct TrickleHandler {
    runtime: Arc<dyn Runtime>,
    ws_out_tx: mpsc::UnboundedSender<String>,
    done_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for TrickleHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        println!(
            "Local ICE candidate: {:?} {}:{}",
            event.candidate.typ, event.candidate.address, event.candidate.port
        );

        if let Ok(candidate_init) = event.candidate.to_json()
            && let Ok(json) = serde_json::to_string(&candidate_init)
        {
            let _ = self.ws_out_tx.send(json);
        }
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        println!("ICE Gathering State has changed: {state}");
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer Connection State has changed: {state}");
        if state == RTCPeerConnectionState::Failed || state == RTCPeerConnectionState::Closed {
            let _ = self.done_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let done_tx = self.done_tx.clone();
        let runtime = self.runtime.clone();
        runtime.spawn(Box::pin(async move {
            let label = dc.label().await.unwrap_or_default();
            let id = dc.id();
            let mut opened = false;
            let mut count = 0u32;
            let mut send_timer = Box::pin(sleep(Duration::from_secs(3)));

            loop {
                if opened {
                    futures::select! {
                        event = dc.poll().fuse() => {
                            match event {
                                Some(DataChannelEvent::OnMessage(msg)) => {
                                    let text = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
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
                            let message = format!("{}-{count}", chrono::Local::now().format("%H:%M:%S"));
                            count += 1;
                            if dc.send_text(&message).await.is_err() {
                                let _ = done_tx.try_send(());
                                break;
                            }
                            send_timer = Box::pin(sleep(Duration::from_secs(3)));
                        }
                    }
                } else {
                    match dc.poll().await {
                        Some(DataChannelEvent::OnOpen) => {
                            println!("Data channel '{label}'-'{id}' open");
                            opened = true;
                            send_timer = Box::pin(sleep(Duration::from_secs(3)));
                        }
                        Some(DataChannelEvent::OnClose) | None => {
                            let _ = done_tx.try_send(());
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }));
    }
}

pub async fn run_example(_cli: TrickleCli, config: TrickleExampleConfig) -> Result<()> {
    let runtime = default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;
    let (done_tx, mut done_rx) = channel::<()>(1);
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    tokio::spawn(run_http_server());

    println!("Open http://localhost:8080 to access this demo");
    println!("Press ctrl-c to stop");

    let ws_listener = TcpListener::bind("0.0.0.0:8081").await?;
    println!("WebSocket server listening on ws://localhost:8081");

    let (tcp_stream, _) = ws_listener.accept().await?;
    let ws_stream = accept_async(tcp_stream).await?;
    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    let (incoming_tx, mut incoming_rx) = mpsc::unbounded_channel::<SignalMessage>();
    let (ws_out_tx, mut ws_out_rx) = mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        while let Some(message) = ws_out_rx.recv().await {
            if ws_sink.send(Message::Text(message.into())).await.is_err() {
                break;
            }
        }
    });

    tokio::spawn(async move {
        while let Some(message) = ws_stream.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    if let Ok(signal) = parse_signal_message(text.as_ref()) {
                        let _ = incoming_tx.send(signal);
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    let mut peer_connection: Option<Box<dyn PeerConnection>> = None;

    loop {
        tokio::select! {
            _ = done_rx.recv() => break,
            _ = ctrlc_rx.recv() => break,
            maybe_signal = incoming_rx.recv() => {
                let Some(signal) = maybe_signal else {
                    break;
                };

                match signal {
                    SignalMessage::Offer(offer) => {
                        if let Some(pc) = peer_connection.take() {
                            let _ = pc.close().await;
                        }

                        let mut media = MediaEngine::default();
                        media.register_default_codecs()?;
                        let registry = register_default_interceptors(Registry::new(), &mut media)?;

                        let pc = PeerConnectionBuilder::new()
                            .with_configuration(
                                RTCConfigurationBuilder::new()
                                    .with_ice_servers(config.ice_servers.clone())
                                    .build(),
                            )
                            .with_media_engine(media)
                            .with_interceptor_registry(registry)
                            .with_handler(Arc::new(TrickleHandler {
                                runtime: runtime.clone(),
                                ws_out_tx: ws_out_tx.clone(),
                                done_tx: done_tx.clone(),
                            }))
                            .with_runtime(runtime.clone())
                            .with_udp_addrs(vec![format!("{}:0", get_local_ip())])
                            .build()
                            .await?;

                        pc.set_remote_description(offer).await?;
                        let answer = pc.create_answer(None).await?;
                        pc.set_local_description(answer).await?;

                        let local_desc = pc
                            .local_description()
                            .await
                            .ok_or_else(|| anyhow::anyhow!("no local description"))?;
                        ws_out_tx.send(serde_json::to_string(&local_desc)?)?;
                        println!("{} answer sent immediately; remaining ICE candidates will trickle", config.name);

                        peer_connection = Some(Box::new(pc));
                    }
                    SignalMessage::IceCandidate(candidate) => {
                        if let Some(pc) = peer_connection.as_ref() {
                            pc.add_ice_candidate(candidate).await?;
                        }
                    }
                }
            }
        }
    }

    if let Some(pc) = peer_connection.take() {
        pc.close().await?;
    }

    Ok(())
}

pub fn init_logging(cli: &TrickleCli) -> Result<()> {
    let log_level = log::LevelFilter::from_str(&cli.log_level)?;

    if cli.debug {
        env_logger::Builder::new()
            .target(if !cli.output_log_file.is_empty() {
                env_logger::Target::Pipe(Box::new(
                    std::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(&cli.output_log_file)?,
                ))
            } else {
                env_logger::Target::Stdout
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

    Ok(())
}

async fn run_http_server() {
    let addr = SocketAddr::from_str("0.0.0.0:8080").unwrap();
    let make_svc = make_service_fn(|_| async {
        Ok::<_, hyper::Error>(service_fn(|req| async move {
            match (req.method(), req.uri().path()) {
                (&Method::GET, "/") | (&Method::GET, "/index.html") => Ok::<_, hyper::Error>(
                    Response::builder()
                        .header("Content-Type", "text/html")
                        .body(Body::from(INDEX_HTML))
                        .unwrap(),
                ),
                _ => Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Not Found"))
                    .unwrap()),
            }
        }))
    });

    let server = Server::bind(&addr).serve(make_svc);
    if let Err(err) = server.await {
        eprintln!("HTTP server error: {err}");
    }
}

fn parse_signal_message(text: &str) -> Result<SignalMessage> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    if value.get("candidate").is_some() {
        Ok(SignalMessage::IceCandidate(serde_json::from_value(value)?))
    } else {
        Ok(SignalMessage::Offer(serde_json::from_value(value)?))
    }
}
