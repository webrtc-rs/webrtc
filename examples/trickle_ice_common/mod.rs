use std::io;
use std::io::Write;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use base64::Engine;
use futures::FutureExt;
use rtc::ice::mdns::MulticastDnsMode;
use rtc::peer_connection::transport::RTCDtlsRole;
use signal::get_local_ip;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCIceCandidateInit, RTCIceGatheringState, RTCIceServer,
    RTCIceTransportPolicy, RTCPeerConnectionIceEvent, RTCPeerConnectionState, Registry,
    SettingEngine, register_default_interceptors,
};
use webrtc::runtime::{AsyncTcpStream, Runtime, Sender, channel, default_runtime, sleep};

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
    pub ice_transport_policy: RTCIceTransportPolicy,
}

#[derive(Debug)]
enum SignalMessage {
    Offer(webrtc::peer_connection::RTCSessionDescription),
    IceCandidate(RTCIceCandidateInit),
}

#[derive(Clone)]
struct TrickleHandler {
    runtime: Arc<dyn Runtime>,
    ws_out_tx: Sender<String>,
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
            let _ = self.ws_out_tx.try_send(json);
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

    let rt = runtime.clone();
    runtime.spawn(Box::pin(run_http_server(rt)));

    println!("Open http://localhost:8080 to access this demo");
    println!("Press ctrl-c to stop");

    let ws_std_listener = std::net::TcpListener::bind("0.0.0.0:8081")?;
    let ws_listener = runtime.wrap_tcp_listener(ws_std_listener)?;
    println!("WebSocket server listening on ws://localhost:8081");

    let (tcp_stream, _) = ws_listener.accept().await?;

    // WS Handshake
    let mut req_buf = [0u8; 2048];
    let n = tcp_stream.read(&mut req_buf).await?;
    let req_str = String::from_utf8_lossy(&req_buf[..n]);
    if let Some(handshake) = ws_handshake_response(&req_str) {
        tcp_stream.write_all(handshake.as_bytes()).await?;
    } else {
        return Err(anyhow::anyhow!("Invalid WebSocket handshake request"));
    }

    let (ws_out_tx, mut ws_out_rx) = channel::<String>(100);
    let (incoming_tx, mut incoming_rx) = channel::<SignalMessage>(100);

    let mut peer_connection: Option<Box<dyn PeerConnection>> = None;

    let stream_read = tcp_stream.clone();
    runtime.spawn(Box::pin(async move {
        loop {
            match read_ws_frame(&stream_read).await {
                Ok(Some(text)) => {
                    if let Ok(signal) = parse_signal_message(&text) {
                        if incoming_tx.send(signal).await.is_err() {
                            break;
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    }));

    let stream_write = tcp_stream.clone();
    runtime.spawn(Box::pin(async move {
        while let Some(msg) = ws_out_rx.recv().await {
            if let Err(_) = write_ws_frame(&stream_write, &msg).await {
                break;
            }
        }
    }));

    loop {
        futures::select! {
            _ = done_rx.recv().fuse() => break,
            _ = ctrlc_rx.recv().fuse() => break,
            maybe_signal = incoming_rx.recv().fuse() => {
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
                        let mut setting_engine = SettingEngine::default();
                        setting_engine.set_answering_dtls_role(RTCDtlsRole::Server)?;
                        setting_engine.set_multicast_dns_mode(MulticastDnsMode::QueryOnly);
                        setting_engine.set_multicast_dns_timeout(Some(Duration::from_secs(10)));

                        let pc = PeerConnectionBuilder::new()
                            .with_configuration(
                                RTCConfigurationBuilder::new()
                                    .with_ice_servers(config.ice_servers.clone())
                                    .with_ice_transport_policy(config.ice_transport_policy)
                                    .build(),
                            )
                            .with_media_engine(media)
                            .with_interceptor_registry(registry)
                            .with_setting_engine(setting_engine)
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
                        ws_out_tx.send(serde_json::to_string(&local_desc)?).await?;
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

async fn run_http_server(runtime: Arc<dyn Runtime>) {
    let addr = SocketAddr::from_str("0.0.0.0:8080").unwrap();
    let std_listener = std::net::TcpListener::bind(addr);
    if let Ok(std_listener) = std_listener {
        if let Ok(listener) = runtime.wrap_tcp_listener(std_listener) {
            while let Ok((stream, _)) = listener.accept().await {
                runtime.spawn(Box::pin(async move {
                    let mut buf = [0u8; 1024];
                    if let Ok(n) = stream.read(&mut buf).await {
                        let req = String::from_utf8_lossy(&buf[..n]);
                        if req.starts_with("GET / ") || req.starts_with("GET /index.html ") {
                            let response = format!(
                                "HTTP/1.1 200 OK\r\n\
                                 Content-Type: text/html\r\n\
                                 Content-Length: {}\r\n\
                                 Connection: close\r\n\r\n\
                                 {}",
                                INDEX_HTML.len(),
                                INDEX_HTML
                            );
                            let _ = stream.write_all(response.as_bytes()).await;
                        } else {
                            let response = "HTTP/1.1 404 Not Found\r\n\
                                            Content-Length: 9\r\n\
                                            Connection: close\r\n\r\n\
                                            Not Found";
                            let _ = stream.write_all(response.as_bytes()).await;
                        }
                    }
                }));
            }
        }
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

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h0 = 0x67452301u32;
    let mut h1 = 0xEFCDAB89u32;
    let mut h2 = 0x98BADCFEu32;
    let mut h3 = 0x10325476u32;
    let mut h4 = 0xC3D2E1F0u32;

    let mut msg = data.to_vec();
    let len_bits = (msg.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() + 8) % 64 != 0 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&len_bits.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for i in 0..80 {
            let (f, k) = if i < 20 {
                ((b & c) | (!b & d), 0x5A827999)
            } else if i < 40 {
                (b ^ c ^ d, 0x6ED9EBA1)
            } else if i < 60 {
                ((b & c) | (b & d) | (c & d), 0x8F1BBCDC)
            } else {
                (b ^ c ^ d, 0xCA62C1D6)
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

fn ws_handshake_response(req: &str) -> Option<String> {
    let mut key = None;
    for line in req.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("sec-websocket-key:") {
            key = Some(trimmed["sec-websocket-key:".len()..].trim().to_string());
            break;
        }
    }
    let key = key?;
    let concatenated = format!("{}{}", key, "258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let hash = sha1(concatenated.as_bytes());
    let accept_key = base64::prelude::BASE64_STANDARD.encode(&hash);
    Some(format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\r\n",
        accept_key
    ))
}

async fn read_exact(stream: &Arc<dyn AsyncTcpStream>, buf: &mut [u8]) -> io::Result<()> {
    let mut read_bytes = 0;
    while read_bytes < buf.len() {
        let n = stream.read(&mut buf[read_bytes..]).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            ));
        }
        read_bytes += n;
    }
    Ok(())
}

async fn read_ws_frame(stream: &Arc<dyn AsyncTcpStream>) -> io::Result<Option<String>> {
    let mut header = [0u8; 2];
    if let Err(e) = read_exact(stream, &mut header).await {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            return Ok(None);
        }
        return Err(e);
    }
    let opcode = header[0] & 0x0F;
    let masked = (header[1] & 0x80) != 0;
    let mut payload_len = (header[1] & 0x7F) as usize;

    if opcode == 8 {
        return Ok(None);
    }

    if payload_len == 126 {
        let mut len_bytes = [0u8; 2];
        if let Err(e) = read_exact(stream, &mut len_bytes).await {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }
            return Err(e);
        }
        payload_len = u16::from_be_bytes(len_bytes) as usize;
    } else if payload_len == 127 {
        let mut len_bytes = [0u8; 8];
        if let Err(e) = read_exact(stream, &mut len_bytes).await {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }
            return Err(e);
        }
        payload_len = u64::from_be_bytes(len_bytes) as usize;
    }

    let mut mask = [0u8; 4];
    if masked {
        if let Err(e) = read_exact(stream, &mut mask).await {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }
            return Err(e);
        }
    }

    let mut payload = vec![0u8; payload_len];
    if let Err(e) = read_exact(stream, &mut payload).await {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            return Ok(None);
        }
        return Err(e);
    }

    if masked {
        for i in 0..payload_len {
            payload[i] ^= mask[i % 4];
        }
    }

    if opcode == 1 {
        Ok(Some(String::from_utf8_lossy(&payload).into_owned()))
    } else {
        Ok(Some("".to_string()))
    }
}

async fn write_ws_frame(stream: &Arc<dyn AsyncTcpStream>, text: &str) -> io::Result<()> {
    let payload = text.as_bytes();
    let mut header = vec![];
    header.push(0x81);

    let len = payload.len();
    if len < 126 {
        header.push(len as u8);
    } else if len <= 65535 {
        header.push(126);
        header.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        header.push(127);
        header.extend_from_slice(&(len as u64).to_be_bytes());
    }

    stream.write_all(&header).await?;
    stream.write_all(payload).await?;
    Ok(())
}
