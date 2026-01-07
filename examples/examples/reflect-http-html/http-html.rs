use actix_web::middleware::Logger;
use actix_web::web::Json;
use actix_web::{get, post, web, App, HttpResponse, HttpServer};
use log::{debug, info, warn, LevelFilter};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::{env, fs, io};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_OPUS};
use webrtc::api::{APIBuilder, API};
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType,
};
use webrtc::rtp_transceiver::PayloadType;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};

/// Represents one active connection in memory
struct StoredConnection {
    peer_connection: Arc<RTCPeerConnection>,
    ice_candidates_channel: Mutex<UnboundedReceiver<Option<RTCIceCandidate>>>,
}

/// Represents shared state between all web-worker threads
pub struct AppState {
    // The API is accessed from multiple actix handling-threads, which is why we need Arc
    api: Arc<API>,

    // The HashMap must be write-locked so that no other threads access them at the same time
    connections: Arc<RwLock<HashMap<Uuid, Arc<StoredConnection>>>>,
}

const PAYLOAD_TYPE_OPUS: PayloadType = 120;

/// Create the WebRTC Media-Engine and Actix-Web-Server
#[actix_rt::main]
async fn main() -> io::Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Debug)
        .filter_module("actix_web", LevelFilter::Debug)
        .filter_module("actix_server", LevelFilter::Info)
        .init();

    // Create a MediaEngine object to configure the supported codec
    let mut media_engine = MediaEngine::default();

    // Set up the available codecs
    media_engine
        .register_codec(
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    ..Default::default()
                },
                payload_type: PAYLOAD_TYPE_OPUS,
                ..Default::default()
            },
            RTPCodecType::Audio,
        )
        .expect("error registering codec");

    // Create a InterceptorRegistry. This is the user configurable RTP/RTCP Pipeline.
    // This provides NACKs, RTCP Reports and other features. If you use `webrtc.NewPeerConnection`
    // this is enabled by default. If you are manually managing You MUST create a InterceptorRegistry
    // for each PeerConnection.
    let mut registry = Registry::new();

    // Use the default set of Interceptors
    registry = register_default_interceptors(registry, &mut media_engine)
        .expect("error registering default interceptors");

    // Create the API object with the MediaEngine
    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    let api = Arc::new(api);
    let connections = Arc::new(RwLock::new(HashMap::new()));

    info!("Starting HTTP server");
    let app_server = HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(web::Data::new(AppState {
                api: api.clone(),
                connections: connections.clone(),
            }))
            .service(index)
            .service(start)
            .service(trickle_in)
            .service(trickle_out)
            .service(stop)
    });

    app_server.workers(2).bind("[::]:8000")?.run().await
}

/// Read and return "client.html"
#[get("/")]
async fn index() -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let path = "examples/examples/reflect-http-html/client.html";
    let cwd = env::current_dir()?;
    let content = fs::read_to_string(path)
        .expect(format!("Cannot read file at {}, cwd is {}", path, cwd.display()).as_str());

    Ok(HttpResponse::Ok().content_type("text/html").body(content))
}

#[derive(Deserialize)]
struct StartRequest {
    request: RTCSessionDescription,
}

#[derive(Serialize)]
struct StartResponse {
    response: RTCSessionDescription,
    connection_id: Uuid,
}

/// Start a WebRTC Connection by taking an SDP offer and returning a Connection-ID and an SDP answer back
#[post("/start")]
async fn start(
    body: Json<StartRequest>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    info!("Start connection");
    debug!(
        "Received SDP: {} {}",
        body.request.sdp_type, body.request.sdp
    );

    // Prepare the WebRTC configuration
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let connection_id = Uuid::new_v4();

    // Create the peer-connection
    let peer_connection = Arc::new(state.api.new_peer_connection(config).await?);

    // Allocate one output-track (we will only handle a single audio-track encoded as opus in this example)
    let output_track = Arc::new(TrackLocalStaticRTP::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            ..Default::default()
        },
        "webrtc-rs-0".to_owned(), // track-id
        "webrtc-rs".to_owned(),   // session-id
    ));

    // Add the output-track to the peer-connection
    let rtp_sender = peer_connection
        .add_track(Arc::clone(&output_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    // spawn a worker for reading rtcp-packets
    tokio::spawn(async move {
        debug!("Starting RTCP-Message Worker");
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((rtcp_pkt, _)) = rtp_sender.read(&mut rtcp_buf).await {
            info!("RTCP-Message: {:?}", rtcp_pkt);
        }

        debug!("Ending RTCP-Message Worker");
    });

    // Configure a Listener for received Tracks
    peer_connection.on_track(Box::new(move |track, _, _| {
        debug!("Received On-Track Event: {:?}", track);

        // Verify track is audio (we will only handle a single opus-encoded audio-track in this example)
        if track.kind() != RTPCodecType::Audio {
            debug!("Ignoring track kind {:?}", track.kind());
            return Box::pin(async {});
        }

        // Verify track is opus
        if track.codec().capability.mime_type.ne(MIME_TYPE_OPUS) {
            debug!("Ignoring track codec {:?}", track.codec());
            return Box::pin(async {});
        }

        // Spawn packet-copy worker
        let output_track = output_track.clone();
        tokio::spawn(async move {
            info!("Starting Packet-Copy-Worker");

            let mut packet_counter: i64 = 0;
            while let Ok((rtp, _)) = track.read_rtp().await {
                if packet_counter % 250 == 0 {
                    debug!("Forwarding packet ({} forwarded total)", packet_counter);
                }
                packet_counter += 1;

                // Send received packet right back
                // In a production application you might instead want to clone and forward this packet to different recipients
                // or even decode its audio-content from opus to raw samples, which could then be manipulated or mixed together
                // and then re-encoded before sending them back
                if let Err(err) = output_track.write_rtp(&rtp).await {
                    warn!("Error copying Packet to Output-Track: {err}");
                    break;
                }
            }

            debug!("Ending Packet-Copy-Worker");
        });

        Box::pin(async {})
    }));

    // we need to explicitly clone the Arcs here (and again below) to be able to move them into the ownership scope of the event-handler
    let peer_connection_closer = peer_connection.clone();
    let connections_closer = state.connections.clone();

    // Configure listener for Peer connection state
    peer_connection.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        info!("Peer Connection State has changed: {s}");

        if s == RTCPeerConnectionState::Failed {
            // Wait until PeerConnection has had no network activity for 30 seconds or another failure.
            // It may be reconnected using an ICE Restart.
            // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
            // Note that the PeerConnection might come back from a PeerConnectionStateDisconnected event.
            info!("Peer Connection has failed, closing");
            let peer_connection_closer = peer_connection_closer.clone();

            tokio::spawn(async move {
                // close the PeerConnection - this will stop all network listeners and end all receiver tasks
                // it will also re-trigger this state-liner with the "Closed" state afterwards
                peer_connection_closer
                    .close()
                    .await
                    .expect("Error closing peer connection");
            });
        } else if s == RTCPeerConnectionState::Closed {
            // Connection has been closed, either through network inactivity and a "failed"-state or
            // voluntarily through the stop-api below
            info!("Peer Connection is closed, removing from storage");

            let connections_closer = connections_closer.clone();
            tokio::spawn(async move {
                // remove the stored connection info from memory, freeing up the PeerConnection and all associated memory
                connections_closer.write().await.remove(&connection_id);
            });

            info!("Peer Connection has been closed and removed");
        }

        Box::pin(async {})
    }));

    // Configure listener for local ice-candidates:
    // this handler will be called when the local ice-agent has gatherd another candidate for communication
    let (ice_tx, ice_rx) = unbounded_channel::<Option<RTCIceCandidate>>();
    peer_connection.on_ice_candidate(Box::new(move |ice_candidate: Option<RTCIceCandidate>| {
        info!("Ice-Candidate found: {ice_candidate:?}");

        // send the ice-candidate into the channel where it will be stored until a (waiting or new) long-poll http request reads it
        ice_tx
            .send(ice_candidate)
            .expect("Error sending Ice-Candidate to channel");

        Box::pin(async {})
    }));

    // Cet the received SDP Offer
    peer_connection
        .set_remote_description(body.request.to_owned())
        .await?;

    // Create an SDP-Answer
    let response = peer_connection.create_answer(None).await?;

    // Set the LocalDescription and start network listeners
    peer_connection
        .set_local_description(response.clone())
        .await?;

    let mut connections = state.connections.write().await;

    // Store the Peer-Connection and the Ice-Candidate channel
    // for further activity under the connection-id
    // Lock the connections HashMap for writing
    connections.insert(
        connection_id,
        Arc::new(StoredConnection {
            peer_connection: peer_connection.clone(),
            ice_candidates_channel: Mutex::new(ice_rx),
        }),
    );

    // Respond with the generated SDP Answer and the connection-id
    debug!("Responding SDP: {} {}", response.sdp_type, response.sdp);
    Ok(HttpResponse::Ok().json(StartResponse {
        response,
        connection_id,
    }))
}

#[derive(Deserialize)]
struct TrickleRequest {
    ice_candidate: RTCIceCandidateInit,
}

/// Receive Ice-Candidate from the client
#[post("/trickle/{connection_id}")]
async fn trickle_in(
    connection_id: web::Path<Uuid>,
    body: Json<TrickleRequest>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    info!("Received Trickle Ice Candidate: {:?}", body.ice_candidate);
    // This code is repeated in all http endpoints but only commented here

    // Lock the connections HashMap for reading
    let connections = state.connections.read().await;
    // get the Option<Arc<StoredConnection>> and clone Option & Arc
    let maybe_connection = connections.get(connection_id.as_ref()).cloned();
    // release the read-lock to that parallel start-requests can acquire the write-lock to add new connections
    drop(connections);

    // at this point the Option and the Arc are cloned, which means that we can continue to use both
    // for the lifetime of this function and the StoredConnection will not be freed in the meantime.

    match maybe_connection {
        Some(connection) => {
            // deliver the ice-candidate to the stored connection
            connection
                .peer_connection
                .add_ice_candidate(body.ice_candidate.to_owned())
                .await?;
            Ok(HttpResponse::Ok().finish())
        }
        None => {
            Ok(HttpResponse::NotFound().body(format!("Unknown Connection-Id {}", connection_id)))
        }
    }
}

#[derive(Serialize)]
struct TrickleGetResponse {
    ice_candidate: RTCIceCandidateInit,
}

/// Send Ice-Candidates to the client via http long-polling
#[get("/trickle/{connection_id}")]
async fn trickle_out(
    connection_id: web::Path<Uuid>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    info!("Received Long-Poll Request for Remote Trickle Ice Candidates");
    let connections = state.connections.read().await;
    let maybe_connection = connections.get(connection_id.as_ref()).cloned();
    drop(connections);

    match maybe_connection {
        Some(connection) => {
            // lock the channels read-end exclusively - this is a mpsc (multi-producer-single-consumer) channel which
            // gurantees that each message will be delivered exactly once.
            let mut receiver = connection.ice_candidates_channel.lock().await;

            // Wait for messages on the channel receiver
            match receiver.recv().await {
                // Successfully received a value from the channel and it did indeed contain an ice-candidate
                Some(Some(ice_candidate)) => Ok(
                    // return the ice-candidate in its serialized form
                    HttpResponse::Ok().json(TrickleGetResponse {
                        ice_candidate: ice_candidate.to_json()?,
                    }),
                ),

                // Successfully received a value from the channel, but it did not contain an ice-candidate
                // this signals the last candidate has been received and no more will follow
                Some(None) => {
                    // close the channel
                    receiver.close();

                    // communicate: no more candidates
                    Ok(HttpResponse::NoContent().finish())
                }

                // Did not receive a value from the channel because it was already closed
                None => {
                    // communicate: no more candidates
                    Ok(HttpResponse::NoContent().finish())
                }
            }
        }
        None => {
            Ok(HttpResponse::NotFound().body(format!("Unknown Connection-Id {}", connection_id)))
        }
    }
}

/// Voluntarily close a running connection
#[post("/stop/{connection_id}")]
async fn stop(
    connection_id: web::Path<Uuid>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let connections = state.connections.read().await;
    let maybe_connection = connections.get(connection_id.as_ref()).cloned();
    drop(connections);

    match maybe_connection {
        Some(connection) => {
            connection.peer_connection.close().await?;
            Ok(HttpResponse::Ok().finish())
        }
        None => {
            Ok(HttpResponse::NotFound().body(format!("Unknown Connection-Id {}", connection_id)))
        }
    }
}
