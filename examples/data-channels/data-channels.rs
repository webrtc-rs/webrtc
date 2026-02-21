use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use std::fs::OpenOptions;
use std::sync::Arc;
use std::time::Duration;
use std::{fs, io::Write, str::FromStr};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::error::Result;
use webrtc::peer_connection::{
    MediaEngine, RTCConfigurationBuilder, RTCIceGatheringState, RTCIceServer,
    RTCPeerConnectionState, RTCSessionDescription, Registry, register_default_interceptors,
};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep};

#[derive(Parser)]
#[command(name = "data-channels")]
#[command(author = "Rusty Rain <y@liu.mx>")]
#[command(version = "0.0.0")]
#[command(about = "An example of Data-Channels", long_about = None)]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    input_sdp_file: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    #[arg(long, default_value_t = format!("0.0.0.0"))]
    host: String,
    #[arg(long, default_value_t = 0)]
    port: u16,
}

#[derive(Clone)]
struct TestHandler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for TestHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        println!("ICE gathering state: {:?}", state);
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer Connection State has changed: {state}");
        if state == RTCPeerConnectionState::Failed {
            println!("Peer Connection has gone to failed exiting");
            let _ = self.done_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, data_channel: Arc<dyn DataChannel>) {
        let runtime = self.runtime.clone();
        self.runtime.spawn(Box::pin(async move {
            let label = match data_channel.label().await {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to get data channel label: {e}");
                    return;
                }
            };
            let id = data_channel.id();
            println!("New DataChannel {label} {id}");

            while let Some(event) = data_channel.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        println!("Data channel '{label}'-'{id}' open. Random messages will now be sent to any connected DataChannels every 5 seconds");
                        let data_channel = data_channel.clone();
                        runtime.spawn(Box::pin(async move {
                            let mut result = Result::<()>::Ok(());
                            while result.is_ok() {
                                let timeout = sleep(Duration::from_secs(5));
                                futures::pin_mut!(timeout);

                                futures::select! {
                                    _ = timeout.fuse() =>{
                                        let message = rtc::shared::util::math_rand_alpha(15);
                                        println!("Sending '{message}'");
                                        result = data_channel.send_text(message.as_str()).await;
                                    }
                                }
                            }
                        }));
                    }
                    DataChannelEvent::OnClose => {
                        println!("Data channel {id} is closed");
                        break;
                    }
                    DataChannelEvent::OnMessage(msg) => {
                        let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();
                        println!("Message from DataChannel '{label}': '{msg_str}'");
                    }
                    _ => {}
                }
            }
        }));
    }
}

fn main() -> anyhow::Result<()> {
    block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let host = cli.host;
    let port = cli.port;
    let input_sdp_file = cli.input_sdp_file;
    let output_log_file = cli.output_log_file;
    let log_level = log::LevelFilter::from_str(&cli.log_level)?;
    if cli.debug {
        env_logger::Builder::new()
            .target(if !output_log_file.is_empty() {
                Target::Pipe(Box::new(
                    OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(output_log_file)?,
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

    // Everything below is the WebRTC-rs API! Thanks for using it ❤️.

    let (done_tx, mut done_rx) = channel::<()>(1);
    let (gather_complete_tx, mut gather_complete_rx) = channel(1);
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;
    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let handler = Arc::new(TestHandler {
        runtime: runtime.clone(),
        gather_complete_tx,
        done_tx,
    });

    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;

    let registry = Registry::new();
    // Use the default set of Interceptors
    let registry = register_default_interceptors(registry, &mut media_engine)?;

    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }])
        .build();

    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(handler)
        .with_runtime(runtime)
        .with_udp_addrs(vec![format!("{host}:{port}")])
        .build()
        .await?;

    // Wait for the offer to be pasted
    let line = if input_sdp_file.is_empty() {
        println!("Please paste offer here:");
        signal::must_read_stdin()?
    } else {
        fs::read_to_string(&input_sdp_file)?
    };
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
    println!("offer: {}", offer);

    // Set the remote SessionDescription
    peer_connection.set_remote_description(offer).await?;

    // Create an answer
    let answer = peer_connection.create_answer(None).await?;

    // Sets the LocalDescription, and starts our UDP listeners
    peer_connection.set_local_description(answer).await?;

    // Block until ICE Gathering is complete, disabling trickle ICE
    // we do this because we only can exchange one signaling message
    // in a production application you should exchange ICE Candidates via OnICECandidate
    let _ = gather_complete_rx.recv().await;

    // Output the answer in base64 so we can paste it in browser
    if let Some(local_desc) = peer_connection.local_description().await {
        println!("answer: {}", local_desc);
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = signal::encode(&json_str);
        println!("{b64}");
    } else {
        println!("generate local_description failed!");
    }

    println!("Press ctrl-c to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => {
            println!("received done signal!");
        }
        _ = ctrlc_rx.recv().fuse() => {
            println!("received ctrl-c signal!");
        }
    };

    peer_connection.close().await?;

    Ok(())
}
