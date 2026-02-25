use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use rtc::interceptor::Registry;
use rtc::media::Sample;
use rtc::media::io::ivf_reader::IVFReader;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_VP8, MediaEngine};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::state::RTCSignalingState;
use rtc::rtp_transceiver::RTCRtpSenderId;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RtpCodecKind};
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodingParameters, RTCRtpEncodingParameters};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Write},
    str::FromStr,
};
use webrtc::error::Error;
use webrtc::media_stream::Track;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{
    Mutex, Notify, Receiver, Runtime, Sender, block_on, channel, default_runtime, interval,
};

#[derive(Clone)]
struct AppState {
    runtime: Arc<dyn Runtime>,
    peer_connection: Arc<dyn PeerConnection>,
    video_file: Arc<Mutex<Option<String>>>,
    gathering_complete: Arc<Mutex<Option<Receiver<()>>>>,
    connection_notify: Notify,
    // Track active video streaming tasks by sender_id
    streaming_tasks: Arc<Mutex<HashMap<RTCRtpSenderId, Sender<()>>>>,
}

static INDEX: &str = "examples/play-from-disk-renegotiation/index.html";
static NOTFOUND: &[u8] = b"Not Found";

/// HTTP status code 404
fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(NOTFOUND.into())
        .unwrap()
}

async fn simple_file_send(filename: &str) -> Result<Response<Body>, hyper::Error> {
    match std::fs::read(filename) {
        Ok(content) => Ok(Response::new(Body::from(content))),
        Err(_) => Ok(not_found()),
    }
}

// HTTP Listener to get ICE Credentials/Candidate from remote Peer
async fn remote_handler(
    req: Request<Body>,
    state: Arc<AppState>,
) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") | (&Method::GET, "/index.html") => simple_file_send(INDEX).await,

        (&Method::POST, "/createPeerConnection") => create_peer_connection(req, state).await,

        (&Method::POST, "/addVideo") => add_video(req, state).await,

        (&Method::POST, "/removeVideo") => remove_video(req, state).await,

        // Return the 404 Not Found for other routes.
        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

// do_signaling exchanges all state of the local PeerConnection and is called
// every time a video is added or removed
async fn do_signaling(
    req: Request<Body>,
    state: Arc<AppState>,
) -> Result<Response<Body>, hyper::Error> {
    let sdp_str = match std::str::from_utf8(&hyper::body::to_bytes(req.into_body()).await?) {
        Ok(s) => s.to_owned(),
        Err(err) => panic!("{}", err),
    };
    let offer = match serde_json::from_str::<RTCSessionDescription>(&sdp_str) {
        Ok(s) => s,
        Err(err) => panic!("{}", err),
    };

    println!("offer: {}", offer);
    if let Err(err) = state.peer_connection.set_remote_description(offer).await {
        panic!("{}", err);
    }

    // Create an answer
    let answer = match state.peer_connection.create_answer(None).await {
        Ok(answer) => answer,
        Err(err) => panic!("{}", err),
    };

    // Sets the LocalDescription, and starts our UDP listeners
    if let Err(err) = state.peer_connection.set_local_description(answer).await {
        panic!("{}", err);
    }

    // Block until ICE Gathering is complete, disabling trickle ICE
    // we do this because we only can exchange one signaling message
    // in a production application you should exchange ICE Candidates via OnICECandidate
    {
        let mut gcr = state.gathering_complete.lock().await;
        if let Some(gather_complete_rx) = &mut *gcr {
            let _ = gather_complete_rx.recv().await;
        }
    }

    let payload = if let Some(local_desc) = state.peer_connection.local_description().await {
        println!("answer: {}", local_desc);
        match serde_json::to_string(&local_desc) {
            Ok(p) => p,
            Err(err) => panic!("{}", err),
        }
    } else {
        panic!("generate local_description failed!");
    };

    let mut response = match Response::builder()
        .header("content-type", "application/json")
        .body(Body::from(payload))
    {
        Ok(res) => res,
        Err(err) => panic!("{}", err),
    };

    *response.status_mut() = StatusCode::OK;
    Ok(response)
}

// Add a single video track
async fn create_peer_connection(
    r: Request<Body>,
    state: Arc<AppState>,
) -> Result<Response<Body>, hyper::Error> {
    println!("PeerConnection has been created");
    do_signaling(r, state).await
}

// Add a single video track
async fn add_video(r: Request<Body>, state: Arc<AppState>) -> Result<Response<Body>, hyper::Error> {
    let video_track: Arc<TrackLocalStaticSample> = Arc::new(
        TrackLocalStaticSample::new(MediaStreamTrack::new(
            format!("webrtc-rs-stream-id-{}", rand::random::<u32>()),
            format!("webrtc-rs-track-id-{}", rand::random::<u32>()),
            format!("webrtc-rs-track-label-{}", rand::random::<u32>()),
            RtpCodecKind::Video,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(rand::random::<u32>()),
                    ..Default::default()
                },
                codec: RTCRtpCodec {
                    mime_type: MIME_TYPE_VP8.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                ..Default::default()
            }],
        ))
        .unwrap(),
    );

    let rtp_sender = match state
        .peer_connection
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal>)
        .await
    {
        Ok(rtp_sender) => rtp_sender,
        Err(err) => panic!("{}", err),
    };

    // Read incoming RTCP packets
    // Before these packets are returned they are processed by interceptors. For things
    // like NACK this needs to be called.
    /*tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
        Result::<()>::Ok(())
    });*/

    let video_file = {
        let vf = state.video_file.lock().await;
        vf.clone()
    };

    if let Some(video_file) = video_file {
        let notify = state.connection_notify.clone();

        // Create a cancellation channel for this streaming task
        let (cancel_tx, cancel_rx) = channel::<()>(1);

        // Store the cancellation sender
        let rtp_sender_id = rtp_sender.id();
        {
            let mut tasks = state.streaming_tasks.lock().await;
            tasks.insert(rtp_sender_id, cancel_tx);
        }

        let streaming_tasks = state.streaming_tasks.clone();
        state.runtime.spawn(Box::pin(async move {
            if let Err(err) =
                write_video_to_track(video_file, rtp_sender_id, video_track, notify, cancel_rx)
                    .await
            {
                eprintln!("video streaming error: {}", err);
            }
            // Remove self from tracking when done
            let mut tasks = streaming_tasks.lock().await;
            tasks.remove(&rtp_sender_id);
        }));
    }

    println!("Video track has been added");
    do_signaling(r, state).await
}

// Remove a single sender
async fn remove_video(
    r: Request<Body>,
    state: Arc<AppState>,
) -> Result<Response<Body>, hyper::Error> {
    let senders = state.peer_connection.get_senders().await;
    if !senders.is_empty() {
        if let Err(err) = state.peer_connection.remove_track(&senders[0]).await {
            panic!("{}", err);
        }
    }

    println!("Video track has been removed");
    do_signaling(r, state).await
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "play-from-disk-renegotiation")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "An example of play-from-disk-renegotiation")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    /// Video file to stream (.ivf containing VP8 or VP9)
    #[arg(short, long)]
    video: Option<String>,
}

// ── Event handler ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct TestHandler {
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
    connected: Arc<AtomicBool>,
    connection_notify: Notify,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for TestHandler {
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
            }
            RTCPeerConnectionState::Failed => {
                self.connected.store(false, Ordering::SeqCst);
                let _ = self.done_tx.try_send(());
            }
            _ => {}
        }
    }

    async fn on_signaling_state_change(&self, state: RTCSignalingState) {
        println!("Signaling State has changed: {state}");
        if state == RTCSignalingState::Stable && self.connected.load(Ordering::SeqCst) {
            self.connection_notify.notify_waiters();
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    block_on(async_main())
}

async fn async_main() -> Result<()> {
    let cli = Cli::parse();
    let log_level = log::LevelFilter::from_str(&cli.log_level)?;
    let video_file = cli.video;

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

    if let Some(video_file) = &video_file {
        if !Path::new(video_file).exists() {
            return Err(anyhow::anyhow!(format!(
                "video file: '{video_file}' not exist"
            )));
        }
    }

    // Everything below is the WebRTC-rs API! Thanks for using it ❤️.

    // Create a MediaEngine object to configure the supported codec
    let mut media_engine = MediaEngine::default();

    media_engine.register_default_codecs()?;

    // Create a InterceptorRegistry. This is the user configurable RTP/RTCP Pipeline.
    // This provides NACKs, RTCP Reports and other features. If you use `webrtc.NewPeerConnection`
    // this is enabled by default. If you are manually managing You MUST create a InterceptorRegistry
    // for each PeerConnection.
    // Use the default set of Interceptors
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)?;

    // Create RTC peer connection configuration

    let config = RTCConfigurationBuilder::new()
        /*TODO: Fix localhost ip 127.0.0.1 takes too long to recv RTCIceGatheringState::Complete,
           when stun:stun.l.google.com:19302 is set #778
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }])*/
        .build();

    // Create the API object with the MediaEngine
    let (done_tx, mut done_rx) = channel::<()>(1);
    let (gather_complete_tx, gather_complete_rx) = channel::<()>(1);
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let connection_notify = Notify::new();
    let handler = Arc::new(TestHandler {
        gather_complete_tx,
        done_tx: done_tx.clone(),
        connected: Arc::new(AtomicBool::new(false)),
        connection_notify: connection_notify.clone(),
    });

    let peer_connection: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .with_handler(handler)
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec![format!("127.0.0.1:0")])
            .build()
            .await?,
    );

    // Create app state
    let app_state = Arc::new(AppState {
        runtime: runtime.clone(),
        peer_connection: Arc::clone(&peer_connection),
        video_file: Arc::new(Mutex::new(video_file)),
        gathering_complete: Arc::new(Mutex::new(Some(gather_complete_rx))),
        connection_notify,
        streaming_tasks: Arc::new(Mutex::new(HashMap::new())),
    });

    let app_state_clone = app_state.clone();
    runtime.spawn(Box::pin(async move {
        println!("Open http://localhost:8080 to access this demo");

        let addr = SocketAddr::from_str("0.0.0.0:8080").unwrap();
        let service = make_service_fn(move |_| {
            let state = app_state_clone.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |req| remote_handler(req, state.clone())))
            }
        });
        let server = Server::bind(&addr).serve(service);
        if let Err(e) = server.await {
            eprintln!("server error: {e}");
        }
    }));

    println!("Press ctrl-c to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => {
            println!("received done signal!");
        }
        _ = ctrlc_rx.recv().fuse() => {
            println!("received ctrl-c signal!");
        }
    }

    peer_connection.close().await?;

    Ok(())
}

// Read a video file from disk and write it to a webrtc.Track
// When the video has been completely read this exits without error
async fn write_video_to_track(
    video_file: String,
    video_sender_id: RTCRtpSenderId,
    video_track: Arc<TrackLocalStaticSample>,
    video_notify_rx: Notify,
    mut cancel_rx: Receiver<()>,
) -> Result<()> {
    // Wait for connection established
    video_notify_rx.notified().await;

    println!("play video from disk file {video_file}");

    // Open a IVF file and start reading using our IVFReader
    let file = File::open(video_file)?;
    let reader = BufReader::new(file);
    let (mut ivf, header) = IVFReader::new(reader)?;

    let ssrc = video_track
        .track()
        .ssrcs()
        .next()
        .ok_or(Error::ErrSenderWithNoSSRCs)?;

    // It is important to use a time.Ticker instead of time.Sleep because
    // * avoids accumulating skew, just calling time.Sleep didn't compensate for the time spent parsing the data
    // * works around latency issues with Sleep
    // Send our video file frame at a time. Pace our sending so we send it at the same speed it should be played back as.
    // This isn't required since the video is timestamped, but we will such much higher loss if we send all at once.
    let sleep_time = Duration::from_millis(
        ((1000 * header.timebase_numerator) / header.timebase_denominator) as u64,
    );
    let mut ticker = interval(sleep_time);
    loop {
        // Check for cancellation
        if cancel_rx.try_recv().is_ok() {
            println!(
                "Video streaming cancelled for sender: {:?}",
                video_sender_id
            );
            break;
        }

        let frame = match ivf.parse_next_frame() {
            Ok((frame, _)) => frame,
            Err(err) => {
                println!("All video frames parsed and sent: {err}");
                break;
            }
        };

        video_track
            .sample_writer(ssrc)
            .write_sample(&Sample {
                data: frame.freeze(),
                duration: Duration::from_secs(1),
                ..Default::default()
            })
            .await?;

        let _ = ticker.tick().await;
    }

    Ok(())
}
