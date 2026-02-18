use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use rtc::peer_connection::configuration::{RTCConfigurationBuilder, RTCIceServer};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::state::{RTCIceGatheringState, RTCPeerConnectionState};
use std::fs::OpenOptions;
use std::sync::Arc;
use std::{io::Write, str::FromStr};
use webrtc::data_channel::DataChannel;
use webrtc::peer_connection::*;
use webrtc::runtime::{Runtime, Sender, channel, default_runtime};

#[derive(Parser)]
#[command(name = "data-channels")]
#[command(author = "Rusty Rain <y@liu.mx>")]
#[command(version = "0.0.0")]
#[command(about = "An example of Data-Channels", long_about = None)]
struct Cli {
    #[arg(short, long)]
    client: bool,
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    input_sdp_file: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    #[arg(long, default_value_t = format!("127.0.0.1"))]
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

    async fn on_data_channel(&self, _data_channel: Arc<DataChannel>) {
        self.runtime.spawn(Box::pin(async move {
            /*let d_label = data_channel.label().to_owned();
            let d_id = data_channel.id();
            println!("New DataChannel {d_label} {d_id}");

            // Register channel opening handling

                let d2 = Arc::clone(&d);
                let d_label2 = d_label.clone();
                let d_id2 = d_id;
                d.on_close(Box::new(move || {
                    println!("Data channel closed");
                    Box::pin(async {})
                }));

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
                data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
                    let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();
                    println!("Message from DataChannel '{d_label}': '{msg_str}'");
                    Box::pin(async {})
                }));*/
        }));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let _host = cli.host;
    let _port = cli.port;
    let _is_client = cli.client;
    let _input_sdp_file = cli.input_sdp_file;
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

    let (done_tx, mut done_rx) = channel::<()>();
    let (gather_complete_tx, mut gather_complete_rx) = channel();
    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let handler = Arc::new(TestHandler {
        runtime: runtime.clone(),
        gather_complete_tx,
        done_tx,
    });

    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }])
        .build();

    let mut peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_handler(handler)
        .with_runtime(runtime)
        .with_udp_addrs(vec!["0.0.0.0:0"])
        .build()
        .await?;

    // Wait for the offer to be pasted
    let line = signal::must_read_stdin()?;
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;

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
