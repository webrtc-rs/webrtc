use std::future::Future;
use std::io::Write;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use anyhow::Result;
use clap::{AppSettings, Arg, Command};
use tokio::sync::Mutex;
use tokio::time::Duration;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::{RTCDataChannel, RTCDataChannelEventHandler};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::math_rand_alpha;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::PeerConnectionEventHandler;

struct ConnectionHandler {
    done_tx: Arc<tokio::sync::mpsc::Sender<()>>,
    close_after: Arc<AtomicI32>,
}

impl PeerConnectionEventHandler for ConnectionHandler {
    // Set the handler for Peer connection state
    // This will notify you when the peer has connected/disconnected
    fn on_peer_connection_state_change(
        &mut self,
        state: RTCPeerConnectionState,
    ) -> impl Future<Output = ()> + Send {
        async move {
            if state == RTCPeerConnectionState::Failed {
                // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
                // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
                // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
                println!("Peer Connection has gone to failed exiting");
                let _ = self.done_tx.try_send(());
            }
        }
    }

    // Register data channel creation handling
    fn on_data_channel(&mut self, channel: Arc<RTCDataChannel>) -> impl Future<Output = ()> + Send {
        async move {
            let d_label = channel.label().to_owned();
            let d_id = channel.id();
            println!("New DataChannel {d_label} {d_id}");

            let (done_tx, done_rx) = tokio::sync::mpsc::channel::<()>(1);
            let done_tx = Arc::new(Mutex::new(Some(done_tx)));

            let dc_handle = DataChannelHandler {
                label: d_label,
                id: d_id,
                done_rx,
                done_tx,
                channel: channel.clone(),
                close_after: self.close_after.clone(),
            };
            channel.with_event_handler(dc_handle);
        }
    }
}

struct DataChannelHandler {
    label: String,
    id: u16,
    done_rx: tokio::sync::mpsc::Receiver<()>,
    done_tx: Arc<Mutex<Option<tokio::sync::mpsc::Sender<()>>>>,
    channel: Arc<RTCDataChannel>,
    close_after: Arc<AtomicI32>,
}

impl RTCDataChannelEventHandler for DataChannelHandler {
    // Register text message handling
    fn on_message(&mut self, msg: DataChannelMessage) -> impl Future<Output = ()> + Send {
        async move {
            let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();
            println!("Message from DataChannel '{}': '{msg_str}'", self.label);
        }
    }

    // Register channel opening handling
    fn on_open(&mut self) -> impl Future<Output = ()> + Send {
        async move {
            println!("Data channel '{}'-'{}' open. Random messages will now be sent to any connected DataChannels every 5 seconds", self.label, self.id);

            let mut result = Result::<usize>::Ok(0);
            while result.is_ok() {
                let timeout = tokio::time::sleep(Duration::from_secs(5));
                tokio::pin!(timeout);

                tokio::select! {
                    _ = self.done_rx.recv() => {
                        break;
                    }
                    _ = timeout.as_mut() =>{
                        let message = math_rand_alpha(15);
                        println!("Sending '{message}'");
                        result = self.channel.send_text(message).await.map_err(Into::into);

                        let cnt = self.close_after.fetch_sub(1, Ordering::SeqCst);
                        if cnt <= 0 {
                            println!("Sent times out. Closing data channel '{}'-'{}'.", self.label, self.id);
                            let _ = self.channel.close().await;
                            break;
                        }
                    }
                };
            }
        }
    }

    // Register channel closing handling
    fn on_close(&mut self) -> impl Future<Output = ()> + Send {
        async move {
            println!("Data channel '{}'-'{}' closed.", self.label, self.id);
            let mut done = self.done_tx.lock().await;
            done.take();
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = Command::new("data-channels-close")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of Data-Channels-Close.")
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
            Arg::new("close-after")
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
    let done_tx = Arc::new(done_tx);

    peer_connection.with_event_handler(ConnectionHandler {
        done_tx,
        close_after: close_after.clone(),
    });

    // Wait for the offer to be pasted
    let line = signal::must_read_stdin()?;
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;

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
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = signal::encode(&json_str);
        println!("{b64}");
    } else {
        println!("generate local_description failed!");
    }

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
