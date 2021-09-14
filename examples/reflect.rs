mod internal;

use anyhow::Result;
use clap::{App, AppSettings, Arg};
use std::sync::Arc;
use tokio::time::Duration;

use interceptor::registry::Registry;
use interceptor::Attributes;
use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_VP8};
use webrtc::api::APIBuilder;
use webrtc::media::rtp::rtp_codec::{RTPCodecCapability, RTPCodecParameters, RTPCodecType};
use webrtc::media::rtp::rtp_receiver::RTPReceiver;
use webrtc::media::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::media::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::media::track::track_remote::TrackRemote;
use webrtc::peer::configuration::Configuration;
use webrtc::peer::ice::ice_server::ICEServer;
use webrtc::peer::peer_connection_state::PeerConnectionState;
use webrtc::peer::sdp::session_description::{SessionDescription, SessionDescriptionSerde};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let mut app = App::new("reflect")
        .version("0.1.0")
        .author("Rain Liu <yuliu@webrtc.rs>")
        .about("An example of how to send back to the user exactly what it receives using the same PeerConnection.")
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::SubcommandsNegateReqs)
        .arg(
            Arg::with_name("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    // Everything below is the WebRTC-rs API! Thanks for using it ❤️.

    // Create a MediaEngine object to configure the supported codec
    let mut m = MediaEngine::default();

    // Setup the codecs you want to use.
    // We'll use a VP8 and Opus but you can also define your own
    m.register_codec(
        RTPCodecParameters {
            capability: RTPCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 96,
            ..Default::default()
        },
        RTPCodecType::Video,
    )?;

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
    let mut peer_connection = api.new_peer_connection(config).await?;

    // Create Track that we send video back to browser on
    let output_track = Arc::new(TrackLocalStaticRTP::new(
        RTPCodecCapability {
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
    let mut offer = SessionDescription::default();
    let line = internal::signal::must_read_stdin()?;
    let desc_data = internal::signal::decode(line.as_str())?;
    offer.serde = serde_json::from_str::<SessionDescriptionSerde>(&desc_data)?;

    // Set the remote SessionDescription
    peer_connection.set_remote_description(offer).await?;

    // Set a handler for when a new remote track starts, this handler copies inbound RTP packets,
    // replaces the SSRC and sends them back
    let rtcp_writer = peer_connection.get_rtcp_writer();
    peer_connection
        .on_track(Box::new(
            move |track: Option<Arc<TrackRemote>>, _receiver: Option<Arc<RTPReceiver>>| {
                if let Some(track) = track {
                    let track2 = Arc::clone(&track);
                    let output_track2 = Arc::clone(&output_track);
                    let rtcp_writer2 = Arc::clone(&rtcp_writer);
                    Box::pin(async move {
                        // Send a PLI on an interval so that the publisher is pushing a keyframe every rtcpPLIInterval
                        // This is a temporary fix until we implement incoming RTCP events, then we would push a PLI only when a viewer requests it
                        let media_ssrc = track2.ssrc();
                        tokio::spawn(async move {
                            let mut result = Result::<usize>::Ok(0);
                            while result.is_ok() {
                                let timeout = tokio::time::sleep(Duration::from_secs(3));
                                tokio::pin!(timeout);

                                tokio::select! {
                                    _ = timeout.as_mut() =>{
                                        let a = Attributes::new();
                                        result = rtcp_writer2.write(&PictureLossIndication{
                                                sender_ssrc: 0,
                                                media_ssrc,
                                        }, &a).await;
                                    }
                                };
                            }
                        });

                        print!(
                            "Track has started, of type {}: {} \n",
                            track2.payload_type(),
                            track2.codec().await.capability.mime_type
                        );
                        // Read RTP packets being sent to webrtc-rs
                        while let Ok((rtp, _)) = track2.read_rtp().await {
                            if let Err(err) = output_track2.write_rtp(&rtp).await {
                                print!("output track write_rtp got error: {}", err);
                                break;
                            }
                        }

                        print!("on_track finished!");
                    })
                } else {
                    Box::pin(async {})
                }
            },
        ))
        .await;

    // Set the handler for Peer connection state
    // This will notify you when the peer has connected/disconnected
    peer_connection
        .on_peer_connection_state_change(Box::new(move |s: PeerConnectionState| {
            print!("Peer Connection State has changed: {}\n", s);

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
        println!("{}", json_str);
        let b64 = internal::signal::encode(&json_str);
        println!("{}", b64);
    } else {
        println!("generate local_description failed!");
    }

    println!("Press ctlr-c to stop server");
    tokio::signal::ctrl_c().await.unwrap();

    peer_connection.close().await?;

    Ok(())
}
