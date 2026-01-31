use std::fs::File;
use std::io::{BufReader, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use clap::{AppSettings, Arg, Command};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use tokio::sync::Mutex;
use tokio::time::Duration;
use tokio_util::codec::{BytesCodec, FramedRead};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_VP8};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::io::ivf_reader::IVFReader;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;
use webrtc::Error;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref PEER_CONNECTION_MUTEX: Arc<Mutex<Option<Arc<RTCPeerConnection>>>> =
        Arc::new(Mutex::new(None));
    static ref VIDEO_FILE: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
}

static INDEX: &str = "examples/examples/play-from-disk-renegotiation/index.html";
static NOTFOUND: &[u8] = b"Not Found";

/// HTTP status code 404
fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(NOTFOUND.into())
        .unwrap()
}

async fn simple_file_send(filename: &str) -> Result<Response<Body>, hyper::Error> {
    // Serve a file by asynchronously reading it by chunks using tokio-util crate.

    if let Ok(file) = tokio::fs::File::open(filename).await {
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);
        return Ok(Response::new(body));
    }

    Ok(not_found())
}

// HTTP Listener to get ICE Credentials/Candidate from remote Peer
async fn remote_handler(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let pc = {
        let pcm = PEER_CONNECTION_MUTEX.lock().await;
        pcm.clone().unwrap()
    };

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") | (&Method::GET, "/index.html") => simple_file_send(INDEX).await,

        (&Method::POST, "/createPeerConnection") => create_peer_connection(&pc, req).await,

        (&Method::POST, "/addVideo") => add_video(&pc, req).await,

        (&Method::POST, "/removeVideo") => remove_video(&pc, req).await,

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
    pc: &Arc<RTCPeerConnection>,
    req: Request<Body>,
) -> Result<Response<Body>, hyper::Error> {
    let sdp_str = match std::str::from_utf8(&hyper::body::to_bytes(req.into_body()).await?) {
        Ok(s) => s.to_owned(),
        Err(err) => panic!("{}", err),
    };
    let offer = match serde_json::from_str::<RTCSessionDescription>(&sdp_str) {
        Ok(s) => s,
        Err(err) => panic!("{}", err),
    };

    if let Err(err) = pc.set_remote_description(offer).await {
        panic!("{}", err);
    }

    // Create channel that is blocked until ICE Gathering is complete
    let mut gather_complete = pc.gathering_complete_promise().await;

    // Create an answer
    let answer = match pc.create_answer(None).await {
        Ok(answer) => answer,
        Err(err) => panic!("{}", err),
    };

    // Sets the LocalDescription, and starts our UDP listeners
    if let Err(err) = pc.set_local_description(answer).await {
        panic!("{}", err);
    }

    // Block until ICE Gathering is complete, disabling trickle ICE
    // we do this because we only can exchange one signaling message
    // in a production application you should exchange ICE Candidates via OnICECandidate
    let _ = gather_complete.recv().await;

    let payload = if let Some(local_desc) = pc.local_description().await {
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
    pc: &Arc<RTCPeerConnection>,
    r: Request<Body>,
) -> Result<Response<Body>, hyper::Error> {
    if pc.connection_state() != RTCPeerConnectionState::New {
        panic!(
            "create_peer_connection called in non-new state ({})",
            pc.connection_state()
        );
    }

    println!("PeerConnection has been created");
    do_signaling(pc, r).await
}

// Add a single video track
async fn add_video(
    pc: &Arc<RTCPeerConnection>,
    r: Request<Body>,
) -> Result<Response<Body>, hyper::Error> {
    let video_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        format!("video-{}", rand::random::<u32>()),
        format!("video-{}", rand::random::<u32>()),
    ));

    let rtp_sender = match pc
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await
    {
        Ok(rtp_sender) => rtp_sender,
        Err(err) => panic!("{}", err),
    };

    // Read incoming RTCP packets
    // Before these packets are returned they are processed by interceptors. For things
    // like NACK this needs to be called.
    tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
        Result::<()>::Ok(())
    });

    let video_file = {
        let vf = VIDEO_FILE.lock().await;
        vf.clone()
    };

    if let Some(video_file) = video_file {
        tokio::spawn(async move {
            let _ = write_video_to_track(video_file, video_track).await;
        });
    }

    println!("Video track has been added");
    do_signaling(pc, r).await
}

// Remove a single sender
async fn remove_video(
    pc: &Arc<RTCPeerConnection>,
    r: Request<Body>,
) -> Result<Response<Body>, hyper::Error> {
    let senders = pc.get_senders().await;
    if !senders.is_empty() {
        if let Err(err) = pc.remove_track(&senders[0]).await {
            panic!("{}", err);
        }
    }

    println!("Video track has been removed");
    do_signaling(pc, r).await
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = Command::new("play-from-disk-renegotiation")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of play-from-disk-renegotiation.")
        .setting(AppSettings::DeriveDisplayOrder)
        .subcommand_negates_reqs(true)
        .arg(
            Arg::new("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        )
        .arg(
            Arg::new("debug")
                .long("debug")
                .short('d')
                .help("Prints debug log information"),
        )
        .arg(
            Arg::new("video")
                .required_unless_present("FULLHELP")
                .takes_value(true)
                .short('v')
                .long("video")
                .help("Video file to be streaming."),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let debug = matches.is_present("debug");
    if debug {
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

    let video_file = matches.value_of("video");

    if let Some(video_file) = video_file {
        if !Path::new(video_file).exists() {
            return Err(Error::new(format!("video file: '{video_file}' not exist")).into());
        }
        let mut vf = VIDEO_FILE.lock().await;
        *vf = Some(video_file.to_owned());
    }

    // Everything below is the WebRTC-rs API! Thanks for using it ❤️.

    // Create a MediaEngine object to configure the supported codec
    let mut m = MediaEngine::default();

    m.register_default_codecs()?;

    // Create a InterceptorRegistry. This is the user configurable RTP/RTCP Pipeline.
    // This provides NACKs, RTCP Reports and other features. If you use `webrtc.NewPeerConnection`
    // this is enabled by default. If you are manually managing You MUST create a InterceptorRegistry
    // for each PeerConnection.
    let mut registry = Registry::new();

    // Use the default set of Interceptors
    registry = register_default_interceptors(registry, &mut m)?;

    // Create the API object with the MediaEngine
    let api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();

    // Prepare the configuration
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Create a new RTCPeerConnection
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Set the handler for Peer connection state
    // This will notify you when the peer has connected/disconnected
    peer_connection.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        println!("Peer Connection State has changed: {s}");

        if s == RTCPeerConnectionState::Failed {
            // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
            // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
            // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
            println!("Peer Connection has gone to failed exiting");
            let _ = done_tx.try_send(());
        }

        Box::pin(async {})
    }));

    {
        let mut pcm = PEER_CONNECTION_MUTEX.lock().await;
        *pcm = Some(Arc::clone(&peer_connection));
    }

    tokio::spawn(async move {
        println!("Open http://localhost:8080 to access this demo");

        let addr = SocketAddr::from_str("0.0.0.0:8080").unwrap();
        let service =
            make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(remote_handler)) });
        let server = Server::bind(&addr).serve(service);
        // Run this server for... forever!
        if let Err(e) = server.await {
            eprintln!("server error: {e}");
        }
    });

    println!("Press ctrl-c to stop");
    tokio::select! {
        _ = done_rx.recv() => {
            println!("received done signal!");
        }
        _ = tokio::signal::ctrl_c() => {
            println!();
        }
    };

    peer_connection.close().await?;

    Ok(())
}

// Read a video file from disk and write it to a webrtc.Track
// When the video has been completely read this exits without error
async fn write_video_to_track(video_file: String, t: Arc<TrackLocalStaticSample>) -> Result<()> {
    println!("play video from disk file {video_file}");

    // Open a IVF file and start reading using our IVFReader
    let file = File::open(video_file)?;
    let reader = BufReader::new(file);
    let (mut ivf, header) = IVFReader::new(reader)?;

    // It is important to use a time.Ticker instead of time.Sleep because
    // * avoids accumulating skew, just calling time.Sleep didn't compensate for the time spent parsing the data
    // * works around latency issues with Sleep
    // Send our video file frame at a time. Pace our sending so we send it at the same speed it should be played back as.
    // This isn't required since the video is timestamped, but we will such much higher loss if we send all at once.
    let sleep_time = Duration::from_millis(
        ((1000 * header.timebase_numerator) / header.timebase_denominator) as u64,
    );
    let mut ticker = tokio::time::interval(sleep_time);
    loop {
        let frame = match ivf.parse_next_frame() {
            Ok((frame, _)) => frame,
            Err(err) => {
                println!("All video frames parsed and sent: {err}");
                return Err(err.into());
            }
        };

        t.write_sample(&Sample {
            data: frame.freeze(),
            duration: Duration::from_secs(1),
            ..Default::default()
        })
        .await?;

        let _ = ticker.tick().await;
    }
}
