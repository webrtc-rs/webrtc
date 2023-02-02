use anyhow::Result;
use clap::{AppSettings, Arg, Command};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use std::io::Write;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tokio_util::codec::{BytesCodec, FramedRead};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref PEER_CONNECTION_MUTEX: Arc<Mutex<Option<Arc<RTCPeerConnection>>>> =
        Arc::new(Mutex::new(None));
}

static INDEX: &str = "examples/examples/ice-restart/index.html";
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
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") | (&Method::GET, "/index.html") => simple_file_send(INDEX).await,

        (&Method::POST, "/doSignaling") => do_signaling(req).await,

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
async fn do_signaling(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let pc = {
        let mut peer_connection = PEER_CONNECTION_MUTEX.lock().await;
        if let Some(pc) = &*peer_connection {
            Arc::clone(pc)
        } else {
            // Create a MediaEngine object to configure the supported codec
            let mut m = MediaEngine::default();

            match m.register_default_codecs() {
                Ok(_) => {}
                Err(err) => panic!("{}", err),
            };

            // Create a InterceptorRegistry. This is the user configurable RTP/RTCP Pipeline.
            // This provides NACKs, RTCP Reports and other features. If you use `webrtc.NewPeerConnection`
            // this is enabled by default. If you are manually managing You MUST create a InterceptorRegistry
            // for each PeerConnection.
            let mut registry = Registry::new();

            // Use the default set of Interceptors
            registry = match register_default_interceptors(registry, &mut m) {
                Ok(r) => r,
                Err(err) => panic!("{}", err),
            };

            // Create the API object with the MediaEngine
            let api = APIBuilder::new()
                .with_media_engine(m)
                .with_interceptor_registry(registry)
                .build();

            // Create a new RTCPeerConnection
            let pc = match api.new_peer_connection(RTCConfiguration::default()).await {
                Ok(p) => p,
                Err(err) => panic!("{}", err),
            };
            let pc = Arc::new(pc);

            // Set the handler for ICE connection state
            // This will notify you when the peer has connected/disconnected
            pc.on_ice_connection_state_change(Box::new(
                |connection_state: RTCIceConnectionState| {
                    println!("ICE Connection State has changed: {connection_state}");
                    Box::pin(async {})
                },
            ));

            // Send the current time via a DataChannel to the remote peer every 3 seconds
            pc.on_data_channel(Box::new(|d: Arc<RTCDataChannel>| {
                Box::pin(async move {
                    let d2 = Arc::clone(&d);
                    d.on_open(Box::new(move || {
                        Box::pin(async move {
                            while d2
                                .send_text(format!("{:?}", tokio::time::Instant::now()))
                                .await
                                .is_ok()
                            {
                                tokio::time::sleep(Duration::from_secs(3)).await;
                            }
                        })
                    }));
                })
            }));

            *peer_connection = Some(Arc::clone(&pc));
            pc
        }
    };

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

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = Command::new("ice-restart")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of ice-restart.")
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
    tokio::signal::ctrl_c().await.unwrap();

    Ok(())
}
