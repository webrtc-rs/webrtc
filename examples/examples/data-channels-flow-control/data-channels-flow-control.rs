use anyhow::Result;
use bytes::Bytes;
use clap::{AppSettings, Arg, Command};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::time::Duration;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

const BUFFERED_AMOUNT_LOW_THRESHOLD: usize = 512 * 1024; // 512 KB
const MAX_BUFFERED_AMOUNT: usize = 1024 * 1024; // 1 MB

async fn set_remote_description(pc: &Arc<RTCPeerConnection>, sdp_str: &str) -> Result<()> {
    let desc = serde_json::from_str::<RTCSessionDescription>(sdp_str)?;

    // Apply the desc as the remote description
    pc.set_remote_description(desc).await?;

    Ok(())
}

async fn create_offerer() -> Result<Arc<RTCPeerConnection>> {
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
            //urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Create a new RTCPeerConnection
    let pc = Arc::new(api.new_peer_connection(config).await?);

    let ordered = Some(false);
    let max_retransmits = Some(0u16);

    let options = Some(RTCDataChannelInit {
        ordered,
        max_retransmits,
        ..Default::default()
    });

    let (send_more_ch_tx, mut send_more_ch_rx) = tokio::sync::mpsc::channel::<()>(1);
    let send_more_ch_tx = Arc::new(send_more_ch_tx);

    // Create a datachannel with label 'data'
    let dc = pc.create_data_channel("data", options).await?;

    // Register channel opening handling

    // no need to downgrade this to Weak, since on_open is FnOnce callback
    let dc2 = Arc::clone(&dc);
    dc.on_open(Box::new(|| {
        println!(
            "OnOpen-{}-{} : Start sending a series of 1024-byte packets as fast as it can",
            dc2.label(),
            dc2.id()
        );

        tokio::spawn(async move {
            let buf = Bytes::from_static(&[0u8; 1024]);
            loop {
                if dc2.send(&buf).await.is_err() {
                    break;
                }

                tokio::time::sleep(Duration::from_micros(1)).await;
                let buffered_amount = dc2.buffered_amount().await;
                if buffered_amount + buf.len() > MAX_BUFFERED_AMOUNT {
                    let _ = send_more_ch_rx.recv().await;
                }
            }
            println!("exit on_open");
        });
        Box::pin(async {})
    }))
    .await;

    // Set BUFFERED_AMOUNT_LOW_THRESHOLD so that we can get notified when
    // we can send more
    dc.set_buffered_amount_low_threshold(BUFFERED_AMOUNT_LOW_THRESHOLD)
        .await;

    // This callback is made when the current bufferedAmount becomes lower than the threshold
    dc.on_buffered_amount_low(Box::new(move || {
        let send_more_ch_tx2 = Arc::clone(&send_more_ch_tx);
        async move {
            let _ = send_more_ch_tx2.send(()).await;
        }
    }))
    .await;

    Ok(pc)
}

async fn create_answerer() -> Result<Arc<RTCPeerConnection>> {
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
            //urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Create a new RTCPeerConnection
    let pc = Arc::new(api.new_peer_connection(config).await?);

    pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
        let total_bytes_received = Arc::new(AtomicUsize::new(0));

        Box::pin(async move {
            // Register channel opening handling
            let total_bytes_received2 = Arc::clone(&total_bytes_received);
            dc.on_open(Box::new(move || {
                println!("OnOpen: Start receiving data"); //, dc2.label(), dc2.id());
                tokio::spawn(async move {
                    let since = SystemTime::now();

                    // Start printing out the observed throughput
                    loop {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        let total_bytes = total_bytes_received2.load(Ordering::SeqCst);
                        let bps = (total_bytes * 8) as f64
                            / SystemTime::now()
                                .duration_since(since)
                                .unwrap()
                                .as_secs_f64();
                        println!("Throughput: {:.03} Mbps", bps / (1024 * 1024) as f64);
                    }
                });

                Box::pin(async {})
            }))
            .await;

            // Register the OnMessage to handle incoming messages
            dc.on_message(Box::new(move |msg: DataChannelMessage| {
                let n = msg.data.len();
                total_bytes_received.fetch_add(n, Ordering::SeqCst);
                Box::pin(async {})
            }))
            .await;
        })
    }))
    .await;

    Ok(pc)
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = Command::new("data-channels-flow-control")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of Data-Channels-Flow-Control.")
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

    {
        let offer_pc = create_offerer().await?;
        let answer_pc = create_answerer().await?;

        // Set ICE Candidate handler. As soon as a PeerConnection has gathered a candidate
        // send it to the other peer
        let offer_pc2 = Arc::downgrade(&offer_pc);
        answer_pc
            .on_ice_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
                let offer_pc3 = offer_pc2.clone();
                Box::pin(async move {
                    if let Some(c) = c {
                        if let Ok(c) = c.to_json().await {
                            if let Some(offer_pc3) = offer_pc3.upgrade() {
                                let _ = offer_pc3.add_ice_candidate(c).await;
                            }
                        }
                    }
                })
            }))
            .await;

        // Set ICE Candidate handler. As soon as a PeerConnection has gathered a candidate
        // send it to the other peer
        let answer_pc2 = Arc::downgrade(&answer_pc);
        offer_pc
            .on_ice_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
                let answer_pc3 = answer_pc2.clone();
                Box::pin(async move {
                    if let Some(c) = c {
                        if let Ok(c) = c.to_json().await {
                            if let Some(answer_pc3) = answer_pc3.upgrade() {
                                let _ = answer_pc3.add_ice_candidate(c).await;
                            }
                        }
                    }
                })
            }))
            .await;

        let (offer_done_tx, mut offer_done_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Set the handler for Peer connection state
        // This will notify you when the peer has connected/disconnected
        offer_pc
            .on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
                println!("Peer Connection State has changed: {} (offerer)", s);

                if s == RTCPeerConnectionState::Failed {
                    // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
                    // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
                    // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
                    println!("Peer Connection (offerer) has gone to failed exiting");
                    let _ = offer_done_tx.try_send(());
                }
                Box::pin(async {})
            }))
            .await;

        let (answer_done_tx, mut answer_done_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Set the handler for Peer connection state
        // This will notify you when the peer has connected/disconnected
        answer_pc
            .on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
                println!("Peer Connection State has changed: {} (answerer)", s);

                if s == RTCPeerConnectionState::Failed {
                    // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
                    // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
                    // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
                    println!("Peer Connection (answerer) has gone to failed exiting");
                    let _ = answer_done_tx.try_send(());
                }
                Box::pin(async {})
            }))
            .await;

        // Now, create an offer
        let offer = offer_pc.create_offer(None).await?;
        let desc = serde_json::to_string(&offer)?;
        offer_pc.set_local_description(offer).await?;
        set_remote_description(&answer_pc, &desc).await?;

        let answer = answer_pc.create_answer(None).await?;
        let desc2 = serde_json::to_string(&answer)?;
        answer_pc.set_local_description(answer).await?;
        set_remote_description(&offer_pc, &desc2).await?;

        println!("Press ctrl-c to stop or wait for 5s");
        let timeout = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(timeout);

        tokio::select! {
            _ = timeout.as_mut() => {}
            _ = tokio::signal::ctrl_c() => {
                println!("");
            }
            _ = offer_done_rx.recv() => {
                println!("received offer done signal!");
            }
            _ = answer_done_rx.recv() => {
                println!("received answer done signal!");
            }
        }

        if let Err(err) = offer_pc.close().await {
            println!("cannot close offer_pc: {}", err);
        }

        if let Err(err) = answer_pc.close().await {
            println!("cannot close answer_pc: {}", err);
        }
    }

    Ok(())
}
