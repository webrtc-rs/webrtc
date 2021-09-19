use anyhow::Result;
use clap::{App, AppSettings, Arg};
use std::sync::Arc;
use tokio::time::Duration;

use interceptor::registry::Registry;
use std::sync::atomic::{AtomicI32, Ordering};
use tokio::sync::Mutex;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data::data_channel::DataChannel;
use webrtc::peer::configuration::Configuration;
use webrtc::peer::ice::ice_server::ICEServer;
use webrtc::peer::peer_connection_state::PeerConnectionState;
use webrtc::peer::sdp::session_description::{SessionDescription, SessionDescriptionSerde};
use webrtc::util::math_rand_alpha;

//use std::io::Write;

#[tokio::main]
async fn main() -> Result<()> {
    /*env_logger::Builder::new()
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
    .init();*/

    let mut app = App::new("data-channels-close")
        .version("0.1.0")
        .author("Rain Liu <yuliu@webrtc.rs>")
        .about("An example of Data-Channels-Close.")
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::SubcommandsNegateReqs)
        .arg(
            Arg::with_name("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        )
        .arg(
            Arg::with_name("close-after")
                .required_unless("FULLHELP")
                .takes_value(true)
                .default_value("5")
                .long("close-after")
                .help("Close data channel after sending X times."),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let close_after = Arc::new(AtomicI32::new(
        matches
            .value_of("close-after")
            .unwrap()
            .to_owned()
            .parse::<i32>()?,
    ));

    // Everything below is the WebRTC-rs API! Thanks for using it ❤️.

    // Create a MediaEngine object to configure the supported codec
    let mut m = MediaEngine::default();

    // Register default codecs
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
    let config = Configuration {
        ice_servers: vec![ICEServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Create a new RTCPeerConnection
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    // Set the handler for Peer connection state
    // This will notify you when the peer has connected/disconnected
    peer_connection
        .on_peer_connection_state_change(Box::new(move |s: PeerConnectionState| {
            println!("Peer Connection State has changed: {}", s);

            if s == PeerConnectionState::Failed {
                // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
                // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
                // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
                println!("Peer Connection has gone to failed exiting");
                std::process::exit(0);
            }

            Box::pin(async {})
        }))
        .await;

    // Register data channel creation handling
    peer_connection
        .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
            let d_label = d.label().to_owned();
            let d_id = d.id();
            println!("New DataChannel {} {}", d_label, d_id);

            let close_after2 = Arc::clone(&close_after);

            // Register channel opening handling
            Box::pin(async move {
                let d2 = Arc::clone(&d);
                let d_label2 = d_label.clone();
                let d_id2 = d_id.clone();
                d.on_open(Box::new(move || {
                    println!("Data channel '{}'-'{}' open. Random messages will now be sent to any connected DataChannels every 5 seconds", d_label2, d_id2);
                    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);
                    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
                    Box::pin(async move {
                        d2.on_close(Box::new(move || {
                            println!("Data channel '{}'-'{}' closed.", d_label2, d_id2);
                            let done_tx2 = Arc::clone(&done_tx);
                            Box::pin(async move{
                                let mut done = done_tx2.lock().await;
                                done.take();
                            })
                        })).await;

                        let mut result = Result::<usize>::Ok(0);
                        while result.is_ok() {
                            let timeout = tokio::time::sleep(Duration::from_secs(5));
                            tokio::pin!(timeout);

                            tokio::select! {
                                _ = done_rx.recv() => {
                                    break;
                                }
                                _ = timeout.as_mut() =>{
                                    let message = math_rand_alpha(15);
                                    println!("Sending '{}'", message);
                                    result = d2.send_text(message).await;

                                    let cnt = close_after2.fetch_sub(1, Ordering::SeqCst);
                                    if cnt <= 0 {
                                        println!("Sent times out. Closing data channel '{}'-'{}'.", d2.label(), d2.id());                                       
                                        let _ = d2.close().await;
                                        break;
                                    }
                                }
                            };
                        }
                    })
                })).await;

                // Register text message handling
                d.on_message(Box::new(move |msg: DataChannelMessage| {
                    let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();
                    print!("Message from DataChannel '{}': '{}'\n", d_label, msg_str);
                    Box::pin(async {})
                })).await;
            })
        }))
        .await;

    // Wait for the offer to be pasted
    let mut offer = SessionDescription::default();
    let line = utilities::must_read_stdin()?;
    let desc_data = utilities::decode(line.as_str())?;
    offer.serde = serde_json::from_str::<SessionDescriptionSerde>(&desc_data)?;

    // Set the remote SessionDescription
    peer_connection.set_remote_description(offer).await?;

    // Create an answer
    let answer = peer_connection.create_answer(None).await?;

    // Create channel that is blocked until ICE Gathering is complete
    let mut gather_complete = peer_connection.gathering_complete_promise().await;

    // Sets the LocalDescription, and starts our UDP listeners
    peer_connection.set_local_description(answer).await?;

    // Block until ICE Gathering is complete, disabling trickle ICE
    // we do this because we only can exchange one signaling message
    // in a production application you should exchange ICE Candidates via OnICECandidate
    let _ = gather_complete.recv().await;

    // Output the answer in base64 so we can paste it in browser
    if let Some(local_desc) = peer_connection.local_description().await {
        let json_str = serde_json::to_string(&local_desc.serde)?;
        let b64 = utilities::encode(&json_str);
        println!("{}", b64);
    } else {
        println!("generate local_description failed!");
    }

    println!("Press ctlr-c to stop");
    tokio::signal::ctrl_c().await.unwrap();

    peer_connection.close().await?;

    Ok(())
}
