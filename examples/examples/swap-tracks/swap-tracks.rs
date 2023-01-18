use anyhow::Result;
use clap::{AppSettings, Arg, Command};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::time::Duration;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_VP8};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::Error;

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = Command::new("swap-tracks")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of swap-tracks.")
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

    // Everything below is the WebRTC-rs API! Thanks for using it ❤️.

    // Create a MediaEngine object to configure the supported codec
    let mut m = MediaEngine::default();

    // Setup the codecs you want to use.
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

    let output_track = Arc::new(TrackLocalStaticRTP::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    // Add this newly created track to the PeerConnection
    let rtp_sender = peer_connection
        .add_track(Arc::clone(&output_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    // Read incoming RTCP packets
    // Before these packets are returned they are processed by interceptors. For things
    // like NACK this needs to be called.
    tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
        Result::<()>::Ok(())
    });

    // Wait for the offer to be pasted
    let line = signal::must_read_stdin()?;
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;

    // Set the remote SessionDescription
    peer_connection.set_remote_description(offer).await?;

    // Which track is currently being handled
    let curr_track = Arc::new(AtomicUsize::new(0));
    // The total number of tracks
    let track_count = Arc::new(AtomicUsize::new(0));
    // The channel of packets with a bit of buffer
    let (packets_tx, mut packets_rx) =
        tokio::sync::mpsc::channel::<webrtc::rtp::packet::Packet>(60);
    let packets_tx = Arc::new(packets_tx);

    // Set a handler for when a new remote track starts, this handler copies inbound RTP packets,
    // replaces the SSRC and sends them back
    let pc = Arc::downgrade(&peer_connection);
    let curr_track1 = Arc::clone(&curr_track);
    let track_count1 = Arc::clone(&track_count);
    peer_connection.on_track(Box::new(move |track, _, _| {
        let track_num = track_count1.fetch_add(1, Ordering::SeqCst);

        let curr_track2 = Arc::clone(&curr_track1);
        let pc2 = pc.clone();
        let packets_tx2 = Arc::clone(&packets_tx);
        tokio::spawn(async move {
            println!(
                "Track has started, of type {}: {}",
                track.payload_type(),
                track.codec().capability.mime_type
            );

            let mut last_timestamp = 0;
            let mut is_curr_track = false;
            while let Ok((mut rtp, _)) = track.read_rtp().await {
                // Change the timestamp to only be the delta
                let old_timestamp = rtp.header.timestamp;
                if last_timestamp == 0 {
                    rtp.header.timestamp = 0
                } else {
                    rtp.header.timestamp -= last_timestamp;
                }
                last_timestamp = old_timestamp;

                // Check if this is the current track
                if curr_track2.load(Ordering::SeqCst) == track_num {
                    // If just switched to this track, send PLI to get picture refresh
                    if !is_curr_track {
                        is_curr_track = true;
                        if let Some(pc) = pc2.upgrade() {
                            if let Err(err) = pc
                                .write_rtcp(&[Box::new(PictureLossIndication {
                                    sender_ssrc: 0,
                                    media_ssrc: track.ssrc(),
                                })])
                                .await
                            {
                                println!("write_rtcp err: {err}");
                            }
                        } else {
                            break;
                        }
                    }
                    let _ = packets_tx2.send(rtp).await;
                } else {
                    is_curr_track = false;
                }
            }

            println!(
                "Track has ended, of type {}: {}",
                track.payload_type(),
                track.codec().capability.mime_type
            );
        });

        Box::pin(async {})
    }));

    let (connected_tx, mut connected_rx) = tokio::sync::mpsc::channel(1);
    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel(1);

    // Set the handler for Peer connection state
    // This will notify you when the peer has connected/disconnected
    peer_connection.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        println!("Peer Connection State has changed: {s}");
        if s == RTCPeerConnectionState::Connected {
            let _ = connected_tx.try_send(());
        } else if s == RTCPeerConnectionState::Failed {
            // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
            // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
            // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
            let _ = done_tx.try_send(());
        }
        Box::pin(async move {})
    }));

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

    // Asynchronously take all packets in the channel and write them out to our
    // track
    tokio::spawn(async move {
        let mut curr_timestamp = 0;
        let mut i = 0;
        while let Some(mut packet) = packets_rx.recv().await {
            // Timestamp on the packet is really a diff, so add it to current
            curr_timestamp += packet.header.timestamp;
            packet.header.timestamp = curr_timestamp;
            // Keep an increasing sequence number
            packet.header.sequence_number = i;
            // Write out the packet, ignoring closed pipe if nobody is listening
            if let Err(err) = output_track.write_rtp(&packet).await {
                if Error::ErrClosedPipe == err {
                    // The peerConnection has been closed.
                    return;
                } else {
                    panic!("{}", err);
                }
            }
            i += 1;
        }
    });

    // Wait for connection, then rotate the track every 5s
    println!("Waiting for connection");
    tokio::select! {
        _ = connected_rx.recv() =>{
            loop {
                println!("Press ctrl-c to stop, or waiting 5 seconds then changing...");
                let timeout = tokio::time::sleep(Duration::from_secs(5));
                tokio::pin!(timeout);

                tokio::select! {
                    _ = timeout.as_mut() => {
                        // We haven't gotten any tracks yet
                        if track_count.load(Ordering::SeqCst) == 0 {
                            continue;
                        }

                        if curr_track.load(Ordering::SeqCst) == track_count.load(Ordering::SeqCst) - 1 {
                            curr_track.store(0, Ordering::SeqCst);
                        } else {
                            curr_track.fetch_add(1, Ordering::SeqCst);
                        }
                        println!(
                            "Switched to track {}",
                            curr_track.load(Ordering::SeqCst) + 1,
                        );
                    }
                    _ = done_rx.recv() => {
                        println!("received done signal!");
                        break;
                    }
                    _ = tokio::signal::ctrl_c() => {
                        println!();
                        break;
                    }
                };
            }
        }
        _ = done_rx.recv() => {}
    };

    peer_connection.close().await?;

    Ok(())
}
