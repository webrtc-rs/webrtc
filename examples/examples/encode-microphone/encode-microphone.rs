use anyhow::Result;
use bytes::Bytes;
use clap::{AppSettings, Arg, Command};
use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use cpal::Device;
use cpal::DevicesError;
use cpal::SampleFormat;
use cpal::SampleRate;
use flume::Sender;
use std::io::Write;
use std::sync::Arc;
use std::thread;
use tokio::sync::Notify;
use tokio::time::Duration;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::audio::buffer::Buffer;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;
use webrtc::Error;

const OGG_PAGE_DURATION: Duration = Duration::from_millis(20);

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = Command::new("encode-microphone")
        .version("0.1.0")
        .author("Mohamad Rajabi <mo.rajbi@gmail.com>")
        .about("An example of getting mic stream and encoding audio using opus.")
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

    let notify_tx = Arc::new(Notify::new());
    let notify_audio = notify_tx.clone();

    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);
    let audio_done_tx = done_tx.clone();

    // Create a audio track
    let audio_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            ..Default::default()
        },
        "audio".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    // Add this newly created track to the PeerConnection
    let rtp_sender = peer_connection
        .add_track(Arc::clone(&audio_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    // Read incoming RTCP packets
    // Before these packets are returned they are processed by interceptors. For things
    // like NACK this needs to be called.
    tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
        Result::<()>::Ok(())
    });

    let (sender, frame_receiver) = flume::bounded::<AudioFrame>(3);
    let (encoded_sender, encoded_receiver) = flume::bounded::<AudioEncodedFrame>(3);

    // Encoder thread
    thread::spawn(move || {
        // We just handle 48khz, to handle other sample rates like 44.1khz you need to use a resampler.
        let mut encoder =
            opus::Encoder::new(48000, opus::Channels::Mono, opus::Application::Voip).unwrap();

        loop {
            let AudioFrame { data } = frame_receiver.recv().unwrap();

            let sample_count = data.len() as u64;
            // sample duration
            let duration = Duration::from_millis(sample_count * 1000 / 48000);
            let encoded = encoder
                .encode_vec_float(&data, 1024)
                .expect("Failed to encode");
            let bytes = Bytes::from(encoded);

            encoded_sender
                .send(AudioEncodedFrame { bytes, duration })
                .unwrap();
        }
    });

    // STREAM
    let device = get_default_input_device().expect("Failed to get default device.");

    // ---
    let input_configs = match device.supported_input_configs() {
        Ok(f) => f,
        Err(e) => {
            panic!("Error getting supported input configs: {:?}", e);
        }
    };
    let input_configs2 = input_configs
        .into_iter()
        .find(|c| c.max_sample_rate() == SampleRate(48000))
        .expect("did not find a sample rate of 48khz");

    let config = input_configs2.with_sample_rate(SampleRate(48000));

    let err_fn = move |err| {
        eprintln!("an error occurred on stream: {}", err);
    };

    // until it is 960
    let mut buffer: Vec<f32> = Vec::new();

    // assume cpal::SampleFormat::F32
    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                for &sample in data {
                    buffer.push(sample.clone());
                    if buffer.len() == 960 {
                        sender
                            .send(AudioFrame {
                                data: Arc::new(buffer.to_owned()),
                            })
                            .expect("Failed to send raw frame to the encoder");
                        // Create a new vec
                        buffer.clear();
                    }
                }
            },
            err_fn,
        )
        .unwrap();

    stream.play().unwrap();

    // SENDER
    tokio::spawn(async move {
        // Wait for connection established
        let _ = notify_audio.notified().await;

        println!("send the audio from the encoder");

        while let Ok(frame) = encoded_receiver.recv_async().await {
            // frame
            audio_track
                .write_sample(&Sample {
                    data: frame.bytes,
                    duration: frame.duration,
                    ..Default::default()
                })
                .await?;
        }

        Result::<()>::Ok(())
    });

    // Set the handler for ICE connection state
    // This will notify you when the peer has connected/disconnected
    peer_connection
        .on_ice_connection_state_change(Box::new(move |connection_state: RTCIceConnectionState| {
            println!("Connection State has changed {}", connection_state);
            if connection_state == RTCIceConnectionState::Connected {
                notify_tx.notify_waiters();
            }
            Box::pin(async {})
        }))
        .await;

    // Set the handler for Peer connection state
    // This will notify you when the peer has connected/disconnected
    peer_connection
        .on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            println!("Peer Connection State has changed: {}", s);

            if s == RTCPeerConnectionState::Failed {
                // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
                // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
                // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
                println!("Peer Connection has gone to failed exiting");
                let _ = done_tx.try_send(());
            }

            Box::pin(async {})
        }))
        .await;

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
        println!("{}", b64);
    } else {
        println!("generate local_description failed!");
    }

    println!("Press ctrl-c to stop");
    tokio::select! {
        _ = done_rx.recv() => {
            println!("received done signal!");
        }
        _ = tokio::signal::ctrl_c() => {
            println!("");
        }
    };

    peer_connection.close().await?;

    Ok(())
}

fn get_default_input_device() -> Result<Device, DevicesError> {
    let device = "default";

    #[cfg(any(
        not(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd")),
        not(feature = "jack")
    ))]
    let host = cpal::default_host();

    // Set up the input device and stream with the default input config.
    let device = if device == "default" {
        host.default_input_device()
    } else {
        host.input_devices()?
            .find(|x| x.name().map(|y| y == device).unwrap_or(false))
    }
    .expect("failed to find input device");

    Ok(device)
}

struct AudioFrame {
    data: Arc<Vec<f32>>,
}

struct AudioEncodedFrame {
    bytes: Bytes,
    duration: Duration,
}
