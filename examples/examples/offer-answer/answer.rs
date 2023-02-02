use anyhow::Result;
use clap::{AppSettings, Arg, Command};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode};
use std::io::Write;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::math_rand_alpha;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref PEER_CONNECTION_MUTEX: Arc<Mutex<Option<Arc<RTCPeerConnection>>>> =
        Arc::new(Mutex::new(None));
    static ref PENDING_CANDIDATES: Arc<Mutex<Vec<RTCIceCandidate>>> = Arc::new(Mutex::new(vec![]));
    static ref ADDRESS: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
}

async fn signal_candidate(addr: &str, c: &RTCIceCandidate) -> Result<()> {
    /*println!(
        "signal_candidate Post candidate to {}",
        format!("http://{}/candidate", addr)
    );*/
    let payload = c.to_json()?.candidate;
    let req = match Request::builder()
        .method(Method::POST)
        .uri(format!("http://{addr}/candidate"))
        .header("content-type", "application/json; charset=utf-8")
        .body(Body::from(payload))
    {
        Ok(req) => req,
        Err(err) => {
            println!("{err}");
            return Err(err.into());
        }
    };

    let _resp = match Client::new().request(req).await {
        Ok(resp) => resp,
        Err(err) => {
            println!("{err}");
            return Err(err.into());
        }
    };
    //println!("signal_candidate Response: {}", resp.status());

    Ok(())
}

// HTTP Listener to get ICE Credentials/Candidate from remote Peer
async fn remote_handler(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let pc = {
        let pcm = PEER_CONNECTION_MUTEX.lock().await;
        pcm.clone().unwrap()
    };
    let addr = {
        let addr = ADDRESS.lock().await;
        addr.clone()
    };

    match (req.method(), req.uri().path()) {
        // A HTTP handler that allows the other WebRTC-rs or Pion instance to send us ICE candidates
        // This allows us to add ICE candidates faster, we don't have to wait for STUN or TURN
        // candidates which may be slower
        (&Method::POST, "/candidate") => {
            //println!("remote_handler receive from /candidate");
            let candidate =
                match std::str::from_utf8(&hyper::body::to_bytes(req.into_body()).await?) {
                    Ok(s) => s.to_owned(),
                    Err(err) => panic!("{}", err),
                };

            if let Err(err) = pc
                .add_ice_candidate(RTCIceCandidateInit {
                    candidate,
                    ..Default::default()
                })
                .await
            {
                panic!("{}", err);
            }

            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::OK;
            Ok(response)
        }

        // A HTTP handler that processes a SessionDescription given to us from the other WebRTC-rs or Pion process
        (&Method::POST, "/sdp") => {
            //println!("remote_handler receive from /sdp");
            let sdp_str = match std::str::from_utf8(&hyper::body::to_bytes(req.into_body()).await?)
            {
                Ok(s) => s.to_owned(),
                Err(err) => panic!("{}", err),
            };
            let sdp = match serde_json::from_str::<RTCSessionDescription>(&sdp_str) {
                Ok(s) => s,
                Err(err) => panic!("{}", err),
            };

            if let Err(err) = pc.set_remote_description(sdp).await {
                panic!("{}", err);
            }

            // Create an answer to send to the other process
            let answer = match pc.create_answer(None).await {
                Ok(a) => a,
                Err(err) => panic!("{}", err),
            };

            /*println!(
                "remote_handler Post answer to {}",
                format!("http://{}/sdp", addr)
            );*/

            // Send our answer to the HTTP server listening in the other process
            let payload = match serde_json::to_string(&answer) {
                Ok(p) => p,
                Err(err) => panic!("{}", err),
            };

            let req = match Request::builder()
                .method(Method::POST)
                .uri(format!("http://{addr}/sdp"))
                .header("content-type", "application/json; charset=utf-8")
                .body(Body::from(payload))
            {
                Ok(req) => req,
                Err(err) => panic!("{}", err),
            };

            let _resp = match Client::new().request(req).await {
                Ok(resp) => resp,
                Err(err) => {
                    println!("{err}");
                    return Err(err);
                }
            };
            //println!("remote_handler Response: {}", resp.status());

            // Sets the LocalDescription, and starts our UDP listeners
            if let Err(err) = pc.set_local_description(answer).await {
                panic!("{}", err);
            }

            {
                let cs = PENDING_CANDIDATES.lock().await;
                for c in &*cs {
                    if let Err(err) = signal_candidate(&addr, c).await {
                        panic!("{}", err);
                    }
                }
            }

            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::OK;
            Ok(response)
        }
        // Return the 404 Not Found for other routes.
        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = Command::new("Answer")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of WebRTC-rs Answer.")
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
            Arg::new("offer-address")
                .takes_value(true)
                .default_value("localhost:50000")
                .long("offer-address")
                .help("Address that the Offer HTTP server is hosted on."),
        )
        .arg(
            Arg::new("answer-address")
                .takes_value(true)
                .default_value("0.0.0.0:60000")
                .long("answer-address")
                .help("Address that the Answer HTTP server is hosted on."),
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

    let offer_addr = matches.value_of("offer-address").unwrap().to_owned();
    let answer_addr = matches.value_of("answer-address").unwrap().to_owned();

    {
        let mut oa = ADDRESS.lock().await;
        *oa = offer_addr.clone();
    }

    // Prepare the configuration
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Create a MediaEngine object to configure the supported codec
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;

    let mut registry = Registry::new();

    // Use the default set of Interceptors
    registry = register_default_interceptors(registry, &mut m)?;

    // Create the API object with the MediaEngine
    let api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();

    // Create a new RTCPeerConnection
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    // When an ICE candidate is available send to the other Pion instance
    // the other Pion instance will add this candidate by calling AddICECandidate
    let pc = Arc::downgrade(&peer_connection);
    let pending_candidates2 = Arc::clone(&PENDING_CANDIDATES);
    let addr2 = offer_addr.clone();
    peer_connection.on_ice_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
        //println!("on_ice_candidate {:?}", c);

        let pc2 = pc.clone();
        let pending_candidates3 = Arc::clone(&pending_candidates2);
        let addr3 = addr2.clone();
        Box::pin(async move {
            if let Some(c) = c {
                if let Some(pc) = pc2.upgrade() {
                    let desc = pc.remote_description().await;
                    if desc.is_none() {
                        let mut cs = pending_candidates3.lock().await;
                        cs.push(c);
                    } else if let Err(err) = signal_candidate(&addr3, &c).await {
                        panic!("{}", err);
                    }
                }
            }
        })
    }));

    println!("Listening on http://{answer_addr}");
    {
        let mut pcm = PEER_CONNECTION_MUTEX.lock().await;
        *pcm = Some(Arc::clone(&peer_connection));
    }

    tokio::spawn(async move {
        let addr = SocketAddr::from_str(&answer_addr).unwrap();
        let service =
            make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(remote_handler)) });
        let server = Server::bind(&addr).serve(service);
        // Run this server for... forever!
        if let Err(e) = server.await {
            eprintln!("server error: {e}");
        }
    });

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

    // Register data channel creation handling
    peer_connection.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        let d_label = d.label().to_owned();
        let d_id = d.id();
        println!("New DataChannel {d_label} {d_id}");

        Box::pin(async move{
            // Register channel opening handling
            let d2 =  Arc::clone(&d);
            let d_label2 = d_label.clone();
            let d_id2 = d_id;
            d.on_open(Box::new(move || {
                println!("Data channel '{d_label2}'-'{d_id2}' open. Random messages will now be sent to any connected DataChannels every 5 seconds");
                Box::pin(async move {
                    let mut result = Result::<usize>::Ok(0);
                    while result.is_ok() {
                        let timeout = tokio::time::sleep(Duration::from_secs(5));
                        tokio::pin!(timeout);

                        tokio::select! {
                            _ = timeout.as_mut() =>{
                                let message = math_rand_alpha(15);
                                println!("Sending '{message}'");
                                result = d2.send_text(message).await.map_err(Into::into);
                            }
                        };
                    }
                })
            }));

            // Register text message handling
            d.on_message(Box::new(move |msg: DataChannelMessage| {
               let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();
               println!("Message from DataChannel '{d_label}': '{msg_str}'");
               Box::pin(async{})
           }));
        })
    }));

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
