use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use rtc::media::Sample;
use rtc::media::io::ogg_reader::{
    OggHeader, OggHeaderType, OggReader, OpusTags, parse_opus_head, parse_opus_tags,
};
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use rtc::peer_connection::configuration::setting_engine::SettingEngine;
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::state::RTCSignalingState;
use rtc::peer_connection::transport::RTCDtlsRole;
use rtc::rtp_transceiver::PayloadType;
use rtc::rtp_transceiver::rtp_sender::RTCRtpCodecParameters;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Duration;
use std::{fs::OpenOptions, io::Write};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::media_stream::Track;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::rtp_transceiver::RtpSender;
use webrtc::runtime::{Notify, Runtime, Sender, block_on, channel, default_runtime, sleep};

const LABEL_AUDIO: &str = "audio";
const LABEL_TRACK: &str = "webrtc-rs";
const WEB_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/play-from-disk-playlist-control/web"
);

#[derive(Parser)]
#[command(name = "play-from-disk-playlist-control")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "A playlist control example streaming Opus from multi-track OGG files.")]
struct Cli {
    #[arg(short, long, default_value_t = format!("127.0.0.1:8080"))]
    addr: String,
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    #[arg(short, long, default_value_t = format!("playlist.ogg"))]
    playlist_file: String,
}

#[derive(Clone)]
struct AppState {
    runtime: Arc<dyn Runtime>,
    tracks: Arc<Vec<OggTrack>>,
}

#[derive(Clone)]
struct SessionHandler {
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
    connected: Arc<AtomicBool>,
    signaling_stable: Arc<AtomicBool>,
    connection_notify: Notify,
}

#[derive(Clone)]
struct BufferedPage {
    payload: Vec<u8>,
    duration: Duration,
    #[allow(dead_code)]
    granule: u64,
}

struct OggTrack {
    serial: u32,
    header: Option<OggHeader>,
    tags: Option<OpusTags>,
    title: String,
    artist: String,
    vendor: String,
    pages: Vec<BufferedPage>,
    runtime: Duration,
}

impl OggTrack {
    fn new(serial: u32) -> Self {
        Self {
            serial,
            header: None,
            tags: None,
            title: format!("serial-{serial}"),
            artist: String::new(),
            vendor: String::new(),
            pages: Vec::new(),
            runtime: Duration::ZERO,
        }
    }
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for SessionHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer Connection State has changed: {state}");
        match state {
            RTCPeerConnectionState::Connected => {
                println!("Peer Connection State has gone to connected!");
                self.connected.store(true, Ordering::SeqCst);
                if self.signaling_stable.load(Ordering::SeqCst) {
                    self.connection_notify.notify_waiters();
                }
            }
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                self.connected.store(false, Ordering::SeqCst);
                let _ = self.done_tx.try_send(());
            }
            _ => {}
        }
    }

    async fn on_signaling_state_change(&self, state: RTCSignalingState) {
        println!("Signaling State has changed: {state}");
        self.signaling_stable
            .store(state == RTCSignalingState::Stable, Ordering::SeqCst);
        if state == RTCSignalingState::Stable && self.connected.load(Ordering::SeqCst) {
            self.connection_notify.notify_waiters();
        }
    }
}

fn main() -> Result<()> {
    block_on(async_main())
}

async fn async_main() -> Result<()> {
    let cli = Cli::parse();
    let log_level = log::LevelFilter::from_str(&cli.log_level)?;

    if cli.debug {
        env_logger::Builder::new()
            .target(if !cli.output_log_file.is_empty() {
                Target::Pipe(Box::new(
                    OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(&cli.output_log_file)?,
                ))
            } else {
                Target::Stdout
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

    if !Path::new(&cli.playlist_file).exists() {
        return Err(anyhow::anyhow!(
            "playlist file: '{}' not exist",
            cli.playlist_file
        ));
    }

    let tracks = Arc::new(parse_playlist(&cli.playlist_file)?);
    if tracks.is_empty() {
        anyhow::bail!("no playable Opus pages were found in {}", cli.playlist_file);
    }

    println!(
        "Loaded {} track(s) from {}",
        tracks.len(),
        cli.playlist_file
    );
    for (i, t) in tracks.iter().enumerate() {
        println!(
            "  [{}] serial={} title={:?} artist={:?} pages={} duration={:?}",
            i + 1,
            t.serial,
            t.title,
            t.artist,
            t.pages.len(),
            t.runtime
        );
    }

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;
    let state = Arc::new(AppState {
        runtime: runtime.clone(),
        tracks,
    });

    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    let addr = cli.addr.clone();
    let server_state = Arc::clone(&state);
    let server_handle = runtime.spawn(Box::pin(async move {
        if let Err(err) = run_http_server(&addr, server_state).await {
            eprintln!("HTTP server error: {err}");
        }
    }));

    println!("Serving UI at http://{} ...", cli.addr);
    println!("Press Ctrl-C to stop");
    let _ = ctrlc_rx.recv().await;
    server_handle.abort();

    Ok(())
}

fn parse_playlist(path: &str) -> Result<Vec<OggTrack>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut ogg_reader = OggReader::new_with_options(reader, false);

    let mut tracks: HashMap<u32, OggTrack> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    let mut last_granule: HashMap<u32, u64> = HashMap::new();

    loop {
        let (payload, page_header) = match ogg_reader.parse_next_page() {
            Ok(result) => result,
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("EOF")
                    || err_str.contains("UnexpectedEof")
                    || err_str.contains("failed to fill")
                {
                    break;
                }
                return Err(e.into());
            }
        };

        let serial = page_header.serial;
        if let std::collections::hash_map::Entry::Vacant(entry) = tracks.entry(serial) {
            entry.insert(OggTrack::new(serial));
            order.push(serial);
        }

        let track = tracks.get_mut(&serial).expect("track must exist");
        if let Some(header_type) = page_header.opus_header_type(&payload) {
            match header_type {
                OggHeaderType::OpusHead => {
                    if let Ok(header) = parse_opus_head(&payload) {
                        track.header = Some(header);
                    }
                    continue;
                }
                OggHeaderType::OpusTags => {
                    if let Ok(tags) = parse_opus_tags(&payload) {
                        for comment in &tags.user_comments {
                            match comment.comment.to_lowercase().as_str() {
                                "title" => track.title = comment.value.clone(),
                                "artist" => track.artist = comment.value.clone(),
                                _ => {}
                            }
                        }
                        if track.vendor.is_empty() {
                            track.vendor = tags.vendor.clone();
                        }
                        track.tags = Some(tags);
                    }
                    continue;
                }
            }
        }

        if track.header.is_none() {
            continue;
        }

        let duration = page_duration(
            track.header.as_ref().expect("header must exist"),
            page_header.granule_position,
            *last_granule.get(&serial).unwrap_or(&0),
        );
        last_granule.insert(serial, page_header.granule_position);

        track.pages.push(BufferedPage {
            payload: payload.to_vec(),
            duration,
            granule: page_header.granule_position,
        });
        track.runtime += duration;
    }

    let mut ordered = Vec::new();
    for serial in order {
        if let Some(mut track) = tracks.remove(&serial) {
            if track.pages.is_empty() {
                continue;
            }
            if track.title.is_empty() || track.title.starts_with("serial-") {
                track.title = format!("Track {}", ordered.len() + 1);
            }
            ordered.push(track);
        }
    }

    Ok(ordered)
}

fn page_duration(header: &OggHeader, granule: u64, last: u64) -> Duration {
    let sample_rate = if header.sample_rate == 0 {
        48_000
    } else {
        header.sample_rate
    };

    if granule <= last {
        return Duration::from_millis(20);
    }

    let sample_count = granule - last;
    if sample_count == 0 {
        return Duration::from_millis(20);
    }

    Duration::from_nanos((sample_count as f64 / sample_rate as f64 * 1_000_000_000.0) as u64)
}

async fn run_http_server(addr: &str, state: Arc<AppState>) -> Result<()> {
    let addr: SocketAddr = addr.parse()?;
    let make_svc = make_service_fn(move |_conn| {
        let state = Arc::clone(&state);
        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                let state = Arc::clone(&state);
                async move { handle_request(req, state).await }
            }))
        }
    });

    Server::bind(&addr).serve(make_svc).await?;
    Ok(())
}

async fn handle_request(
    req: Request<Body>,
    state: Arc<AppState>,
) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/whep") => create_session(req, state).await,
        (&Method::GET, path) => serve_static(path).await,
        _ => Ok(not_found()),
    }
}

async fn create_session(
    req: Request<Body>,
    state: Arc<AppState>,
) -> Result<Response<Body>, hyper::Error> {
    let offer_sdp =
        String::from_utf8_lossy(&hyper::body::to_bytes(req.into_body()).await?).to_string();
    if offer_sdp.trim().is_empty() {
        let mut response = Response::new(Body::from("empty SDP"));
        *response.status_mut() = StatusCode::BAD_REQUEST;
        return Ok(response);
    }

    match handle_whep_connection(
        offer_sdp,
        Arc::clone(&state.tracks),
        Arc::clone(&state.runtime),
    )
    .await
    {
        Ok(answer_sdp) => {
            let mut response = Response::new(Body::from(answer_sdp));
            response
                .headers_mut()
                .insert("Content-Type", "application/sdp".parse().unwrap());
            Ok(response)
        }
        Err(err) => {
            let mut response = Response::new(Body::from(err.to_string()));
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            Ok(response)
        }
    }
}

async fn serve_static(path: &str) -> Result<Response<Body>, hyper::Error> {
    let file_path = if path == "/" {
        format!("{WEB_DIR}/index.html")
    } else {
        format!("{WEB_DIR}{path}")
    };

    match std::fs::read(&file_path) {
        Ok(content) => {
            let content_type = if file_path.ends_with(".html") {
                "text/html"
            } else if file_path.ends_with(".css") {
                "text/css"
            } else if file_path.ends_with(".js") {
                "application/javascript"
            } else {
                "application/octet-stream"
            };

            let mut response = Response::new(Body::from(content));
            response
                .headers_mut()
                .insert("Content-Type", content_type.parse().unwrap());
            Ok(response)
        }
        Err(_) => Ok(not_found()),
    }
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("not found"))
        .unwrap()
}

async fn handle_whep_connection(
    offer_sdp: String,
    tracks: Arc<Vec<OggTrack>>,
    runtime: Arc<dyn Runtime>,
) -> Result<String> {
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_answering_dtls_role(RTCDtlsRole::Server)?;

    let mut media_engine = MediaEngine::default();
    let opus_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            clock_rate: 48_000,
            channels: 2,
            sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 111,
        ..Default::default()
    };
    media_engine.register_codec(opus_codec.clone(), RtpCodecKind::Audio)?;
    let registry =
        register_default_interceptors(rtc::interceptor::Registry::new(), &mut media_engine)?;

    let config = RTCConfigurationBuilder::new().build();

    let (done_tx, mut done_rx) = channel::<()>(1);
    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>(1);
    let connected = Arc::new(AtomicBool::new(false));
    let signaling_stable = Arc::new(AtomicBool::new(false));
    let connection_notify = Notify::new();
    let handler = Arc::new(SessionHandler {
        gather_complete_tx,
        done_tx: done_tx.clone(),
        connected: Arc::clone(&connected),
        signaling_stable: Arc::clone(&signaling_stable),
        connection_notify: connection_notify.clone(),
    });

    let peer_connection: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_setting_engine(setting_engine)
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .with_handler(handler)
            .with_runtime(Arc::clone(&runtime))
            .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
            .build()
            .await?,
    );

    let ssrc = rand::random::<u32>();
    let audio_track = Arc::new(TrackLocalStaticSample::new(MediaStreamTrack::new(
        "webrtc-rs-stream-id".to_string(),
        LABEL_AUDIO.to_string(),
        LABEL_TRACK.to_string(),
        RtpCodecKind::Audio,
        vec![RTCRtpEncodingParameters {
            rtp_coding_parameters: RTCRtpCodingParameters {
                ssrc: Some(ssrc),
                ..Default::default()
            },
            codec: opus_codec.rtp_codec.clone(),
            ..Default::default()
        }],
    ))?);
    let audio_sender = peer_connection
        .add_track(Arc::clone(&audio_track) as Arc<dyn TrackLocal>)
        .await?;

    let playlist_channel = peer_connection
        .create_data_channel("playlist", None)
        .await?;

    let offer = RTCSessionDescription::offer(offer_sdp)?;
    println!("Received Offer {offer}");
    peer_connection.set_remote_description(offer).await?;
    let answer = peer_connection.create_answer(None).await?;
    peer_connection.set_local_description(answer).await?;
    let _ = gather_complete_rx.recv().await;

    let answer_sdp = peer_connection
        .local_description()
        .await
        .map(|desc| desc.sdp)
        .unwrap_or_default();
    println!("Created answer, starting playlist session");

    let current_track = Arc::new(AtomicI32::new(0));
    let switch_track = Arc::new(AtomicI32::new(-1));

    let dc_tracks = Arc::clone(&tracks);
    let dc_channel = Arc::clone(&playlist_channel);
    let dc_current_track = Arc::clone(&current_track);
    let dc_switch_track = Arc::clone(&switch_track);
    runtime.spawn(Box::pin(async move {
        run_playlist_data_channel(dc_channel, dc_tracks, dc_current_track, dc_switch_track).await;
    }));

    let stream_pc = Arc::clone(&peer_connection);
    let stream_tracks = Arc::clone(&tracks);
    let stream_track = Arc::clone(&audio_track);
    let stream_channel = Arc::clone(&playlist_channel);
    let stream_notify = connection_notify.clone();
    let stream_current_track = Arc::clone(&current_track);
    let stream_switch_track = Arc::clone(&switch_track);
    let stream_done_tx = done_tx.clone();
    let stream_connected = Arc::clone(&connected);
    let stream_signaling_stable = Arc::clone(&signaling_stable);
    let payload_type = negotiated_payload_type(&audio_sender).await?;
    runtime.spawn(Box::pin(async move {
        while !(stream_connected.load(Ordering::SeqCst)
            && stream_signaling_stable.load(Ordering::SeqCst))
        {
            stream_notify.notified().await;
        }
        if let Err(err) = stream_playlist_audio(
            stream_track,
            payload_type,
            stream_tracks,
            stream_channel,
            stream_current_track,
            stream_switch_track,
        )
        .await
        {
            eprintln!("playlist streaming error: {err}");
            let _ = stream_done_tx.try_send(());
        }
        let _ = stream_pc;
    }));

    let close_pc = Arc::clone(&peer_connection);
    runtime.spawn(Box::pin(async move {
        let _ = done_rx.recv().await;
        if let Err(err) = close_pc.close().await {
            eprintln!("peer connection close error: {err}");
        }
    }));

    Ok(answer_sdp)
}

async fn run_playlist_data_channel(
    playlist_channel: Arc<dyn DataChannel>,
    tracks: Arc<Vec<OggTrack>>,
    current_track: Arc<AtomicI32>,
    switch_track: Arc<AtomicI32>,
) {
    while let Some(evt) = playlist_channel.poll().await {
        match evt {
            DataChannelEvent::OnOpen => {
                let msg = build_playlist_message(&tracks, current_track.load(Ordering::SeqCst));
                let _ = playlist_channel.send_text(&msg).await;
            }
            DataChannelEvent::OnMessage(msg) => {
                let command = String::from_utf8_lossy(&msg.data).trim().to_lowercase();
                handle_playlist_command(
                    &command,
                    &tracks,
                    &current_track,
                    &switch_track,
                    &playlist_channel,
                )
                .await;
            }
            DataChannelEvent::OnClose | DataChannelEvent::OnError => break,
            _ => {}
        }
    }
}

// Resolve the payload type negotiated for the sender's (single) codec. write_sample stamps
// this on every packet, and rtc's write_rtp requires it to match a negotiated sender codec.
async fn negotiated_payload_type(sender: &Arc<dyn RtpSender>) -> Result<PayloadType> {
    sender
        .get_parameters()
        .await?
        .rtp_parameters
        .codecs
        .first()
        .map(|codec| codec.payload_type)
        .ok_or_else(|| anyhow::anyhow!("sender has no negotiated codec"))
}

async fn stream_playlist_audio(
    audio_track: Arc<TrackLocalStaticSample>,
    payload_type: PayloadType,
    tracks: Arc<Vec<OggTrack>>,
    playlist_channel: Arc<dyn DataChannel>,
    current_track: Arc<AtomicI32>,
    switch_track: Arc<AtomicI32>,
) -> Result<()> {
    let ssrc = *audio_track
        .ssrcs()
        .await
        .first()
        .ok_or(webrtc::error::Error::ErrSenderWithNoSSRCs)?;

    let mut page_idx = 0usize;
    loop {
        let track_idx =
            normalize_index(current_track.load(Ordering::SeqCst), tracks.len() as i32) as usize;
        let track = &tracks[track_idx];

        if page_idx >= track.pages.len() {
            let next = wrap_next(track_idx as i32, tracks.len() as i32);
            current_track.store(next, Ordering::SeqCst);
            page_idx = 0;

            let now_msg = build_now_playing_message(&tracks, next as usize);
            let _ = playlist_channel.send_text(&now_msg).await;
            continue;
        }

        let page = &track.pages[page_idx];
        audio_track
            .sample_writer(ssrc, payload_type)
            .write_sample(&Sample {
                data: page.payload.clone().into(),
                duration: page.duration,
                ..Default::default()
            })
            .await?;

        let switch = switch_track.swap(-1, Ordering::SeqCst);
        if switch >= 0 && (switch as usize) < tracks.len() {
            current_track.store(switch, Ordering::SeqCst);
            page_idx = 0;
            let now_msg = build_now_playing_message(&tracks, switch as usize);
            let _ = playlist_channel.send_text(&now_msg).await;
        } else {
            page_idx += 1;
        }

        sleep(if page.duration.is_zero() {
            Duration::from_millis(20)
        } else {
            page.duration
        })
        .await;
    }
}

async fn handle_playlist_command(
    command: &str,
    tracks: &[OggTrack],
    current_track: &AtomicI32,
    switch_track: &AtomicI32,
    playlist_channel: &Arc<dyn DataChannel>,
) {
    let limit = tracks.len() as i32;
    let current = current_track.load(Ordering::SeqCst);
    let mut next = -1i32;

    match command {
        "next" | "n" | "forward" => next = wrap_next(current, limit),
        "prev" | "previous" | "p" | "back" => next = wrap_prev(current, limit),
        "list" => {
            let msg = build_playlist_message(tracks, current);
            let _ = playlist_channel.send_text(&msg).await;
            return;
        }
        _ => {
            if let Ok(idx) = command.parse::<i32>() {
                next = normalize_index(idx - 1, limit);
            }
        }
    }

    if next < 0 || next == current {
        return;
    }

    switch_track.store(next, Ordering::SeqCst);
    let msg = build_playlist_message(tracks, next);
    let _ = playlist_channel.send_text(&msg).await;
}

fn build_playlist_message(tracks: &[OggTrack], current: i32) -> String {
    let mut msg = format!(
        "playlist|{}\n",
        normalize_index(current, tracks.len() as i32)
    );

    for (i, t) in tracks.iter().enumerate() {
        msg.push_str(&format!(
            "track|{}|{}|{}|{}|{}\n",
            i,
            t.serial,
            t.runtime.as_millis(),
            clean_text(&t.title),
            clean_text(&t.artist),
        ));
    }

    if !tracks.is_empty() {
        let idx = normalize_index(current, tracks.len() as i32) as usize;
        msg.push_str(&build_now_line(&tracks[idx], idx));
    }

    msg
}

fn build_now_playing_message(tracks: &[OggTrack], index: usize) -> String {
    if index >= tracks.len() {
        return String::new();
    }
    build_now_line(&tracks[index], index)
}

fn build_now_line(track: &OggTrack, index: usize) -> String {
    let comments = track
        .tags
        .as_ref()
        .map(|tags| {
            tags.user_comments
                .iter()
                .map(|c| format!("{}={}", clean_text(&c.comment), clean_text(&c.value)))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();

    let channels = track.header.as_ref().map(|h| h.channels).unwrap_or(2);
    let sample_rate = track
        .header
        .as_ref()
        .map(|h| h.sample_rate)
        .unwrap_or(48_000);

    format!(
        "now|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
        index,
        track.serial,
        channels,
        sample_rate,
        track.runtime.as_millis(),
        clean_text(&track.title),
        clean_text(&track.artist),
        clean_text(&track.vendor),
        comments,
    )
}

fn clean_text(v: &str) -> String {
    v.replace('\n', " ").replace('|', "/")
}

fn wrap_next(current: i32, limit: i32) -> i32 {
    if limit == 0 { 0 } else { (current + 1) % limit }
}

fn wrap_prev(current: i32, limit: i32) -> i32 {
    if limit == 0 {
        0
    } else if current == 0 {
        limit - 1
    } else {
        current - 1
    }
}

fn normalize_index(i: i32, limit: i32) -> i32 {
    if limit == 0 || i < 0 {
        0
    } else if i >= limit {
        limit - 1
    } else {
        i
    }
}
