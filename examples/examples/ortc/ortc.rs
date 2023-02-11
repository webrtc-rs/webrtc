use std::io::Write;
use std::sync::Arc;

use anyhow::Result;
use clap::{AppSettings, Arg, Command};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::time::Duration;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::data_channel_parameters::DataChannelParameters;
use webrtc::data_channel::RTCDataChannel;
use webrtc::dtls_transport::dtls_parameters::DTLSParameters;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_gatherer::RTCIceGatherOptions;
use webrtc::ice_transport::ice_parameters::RTCIceParameters;
use webrtc::ice_transport::ice_role::RTCIceRole;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::math_rand_alpha;
use webrtc::sctp_transport::sctp_transport_capabilities::SCTPTransportCapabilities;

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = Command::new("ortc")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of ORTC.")
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
            Arg::new("offer")
                .long("offer")
                .help("Act as the offerer if set."),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let is_offer = matches.is_present("offer");
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

    // Everything below is the Pion WebRTC (ORTC) API! Thanks for using it ❤️.

    // Prepare ICE gathering options
    let ice_options = RTCIceGatherOptions {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Create an API object
    let api = APIBuilder::new().build();

    // Create the ICE gatherer
    let gatherer = Arc::new(api.new_ice_gatherer(ice_options)?);

    // Construct the ICE transport
    let ice = Arc::new(api.new_ice_transport(Arc::clone(&gatherer)));

    // Construct the DTLS transport
    let dtls = Arc::new(api.new_dtls_transport(Arc::clone(&ice), vec![])?);

    // Construct the SCTP transport
    let sctp = Arc::new(api.new_sctp_transport(Arc::clone(&dtls))?);

    let done = Arc::new(Notify::new());
    let done_answer = done.clone();
    let done_offer = done.clone();

    // Handle incoming data channels
    sctp.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        let d_label = d.label().to_owned();
        let d_id = d.id();
        println!("New DataChannel {d_label} {d_id}");

        let done_answer1 = done_answer.clone();
        // Register the handlers
        Box::pin(async move {
            // no need to downgrade this to Weak, since on_open is FnOnce callback
            let d2 = Arc::clone(&d);
            let done_answer2 = done_answer1.clone();
            d.on_open(Box::new(move || {
                Box::pin(async move {
                    tokio::select! {
                        _ = done_answer2.notified() => {
                            println!("received done_answer signal!");
                        }
                        _ = handle_on_open(d2) => {}
                    };

                    println!("exit data answer");
                })
            }));

            // Register text message handling
            d.on_message(Box::new(move |msg: DataChannelMessage| {
                let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();
                println!("Message from DataChannel '{d_label}': '{msg_str}'");
                Box::pin(async {})
            }));
        })
    }));

    let (gather_finished_tx, mut gather_finished_rx) = tokio::sync::mpsc::channel::<()>(1);
    let mut gather_finished_tx = Some(gather_finished_tx);
    gatherer.on_local_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
        if c.is_none() {
            gather_finished_tx.take();
        }
        Box::pin(async {})
    }));

    // Gather candidates
    gatherer.gather().await?;

    let _ = gather_finished_rx.recv().await;

    let ice_candidates = gatherer.get_local_candidates().await?;

    let ice_parameters = gatherer.get_local_parameters().await?;

    let dtls_parameters = dtls.get_local_parameters()?;

    let sctp_capabilities = sctp.get_capabilities();

    let local_signal = Signal {
        ice_candidates,
        ice_parameters,
        dtls_parameters,
        sctp_capabilities,
    };

    // Exchange the information
    let json_str = serde_json::to_string(&local_signal)?;
    let b64 = signal::encode(&json_str);
    println!("{b64}");

    let line = signal::must_read_stdin()?;
    let json_str = signal::decode(line.as_str())?;
    let remote_signal = serde_json::from_str::<Signal>(&json_str)?;

    let ice_role = if is_offer {
        RTCIceRole::Controlling
    } else {
        RTCIceRole::Controlled
    };

    ice.set_remote_candidates(&remote_signal.ice_candidates)
        .await?;

    // Start the ICE transport
    ice.start(&remote_signal.ice_parameters, Some(ice_role))
        .await?;

    // Start the DTLS transport
    dtls.start(remote_signal.dtls_parameters).await?;

    // Start the SCTP transport
    sctp.start(remote_signal.sctp_capabilities).await?;

    // Construct the data channel as the offerer
    if is_offer {
        let id = 1u16;

        let dc_params = DataChannelParameters {
            label: "Foo".to_owned(),
            negotiated: Some(id),
            ..Default::default()
        };

        let d = Arc::new(api.new_data_channel(Arc::clone(&sctp), dc_params).await?);

        // Register the handlers
        // channel.OnOpen(handleOnOpen(channel)) // TODO: OnOpen on handle ChannelAck
        // Temporary alternative

        // no need to downgrade this to Weak
        let d2 = Arc::clone(&d);
        tokio::spawn(async move {
            tokio::select! {
                _ = done_offer.notified() => {
                    println!("received done_offer signal!");
                }
                _ = handle_on_open(d2) => {}
            };

            println!("exit data offer");
        });

        let d_label = d.label().to_owned();
        d.on_message(Box::new(move |msg: DataChannelMessage| {
            let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();
            println!("Message from DataChannel '{d_label}': '{msg_str}'");
            Box::pin(async {})
        }));
    }

    println!("Press ctrl-c to stop");
    tokio::signal::ctrl_c().await.unwrap();
    done.notify_waiters();

    sctp.stop().await?;
    dtls.stop().await?;
    ice.stop().await?;

    Ok(())
}

// Signal is used to exchange signaling info.
// This is not part of the ORTC spec. You are free
// to exchange this information any way you want.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Signal {
    #[serde(rename = "iceCandidates")]
    ice_candidates: Vec<RTCIceCandidate>, //   `json:"iceCandidates"`

    #[serde(rename = "iceParameters")]
    ice_parameters: RTCIceParameters, //    `json:"iceParameters"`

    #[serde(rename = "dtlsParameters")]
    dtls_parameters: DTLSParameters, //   `json:"dtlsParameters"`

    #[serde(rename = "sctpCapabilities")]
    sctp_capabilities: SCTPTransportCapabilities, // `json:"sctpCapabilities"`
}

async fn handle_on_open(d: Arc<RTCDataChannel>) -> Result<()> {
    println!("Data channel '{}'-'{}' open. Random messages will now be sent to any connected DataChannels every 5 seconds", d.label(), d.id());

    let mut result = Result::<usize>::Ok(0);
    while result.is_ok() {
        let timeout = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(timeout);

        tokio::select! {
            _ = timeout.as_mut() =>{
                let message = math_rand_alpha(15);
                println!("Sending '{message}'");
                result = d.send_text(message).await.map_err(Into::into);
            }
        };
    }

    Ok(())
}
