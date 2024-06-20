use anyhow::Result;
use bytes::BytesMut;
use clap::{AppSettings, Arg, Command};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::sync::{Arc, Weak};
use tokio::sync::{mpsc, Notify};
use tokio::time::Duration;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_HEVC, MIME_TYPE_OPUS};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::io::ogg_reader::OggReader;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp::codecs::h264::ANNEXB_NALUSTART_CODE;
use webrtc::rtp::codecs::h265::{H265NALUHeader, H265Packet, H265Payload, UnitType};
use webrtc::rtp::packetizer::Depacketizer;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
use webrtc::rtp_transceiver::rtp_receiver::RTCRtpReceiver;
use webrtc::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use webrtc::rtp_transceiver::{RTCRtpTransceiver, RTCRtpTransceiverInit};
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;
use webrtc::track::track_remote::TrackRemote;
use webrtc::Error;

const OGG_PAGE_DURATION: Duration = Duration::from_millis(20);

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = Command::new("play-from-disk-hevc")
        .version("0.1.0")
        .author("RobinShi <ftaft2000@msn.com>")
        .about("An example of play-from-disk-hevc.")
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
            Arg::new("video")
                .required_unless_present("FULLHELP")
                .takes_value(true)
                .short('v')
                .long("video")
                .help("Video file to be streaming."),
        )
        .arg(
            Arg::new("audio")
                .takes_value(true)
                .short('a')
                .long("audio")
                .help("Audio file to be streaming."),
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

    let video_file = matches.value_of("video");
    let audio_file = matches.value_of("audio");

    if let Some(video_path) = &video_file {
        if !Path::new(video_path).exists() {
            return Err(Error::new(format!("video file: '{video_path}' not exist")).into());
        }
    }
    if let Some(audio_path) = &audio_file {
        if !Path::new(audio_path).exists() {
            return Err(Error::new(format!("audio file: '{audio_path}' not exist")).into());
        }
    }
    let video_file = video_file.map(|v| v.to_owned()).unwrap();
    let audio_file = audio_file.map(|v| v.to_owned()).unwrap();

    let video_file1 = video_file.clone();
    let (offer_sdr, mut offer_rcv) = mpsc::channel::<RTCSessionDescription>(10);
    let (answer_sdr, answer_rcv) = mpsc::channel::<RTCSessionDescription>(10);
    tokio::spawn(async move {
        if let Err(e) = offer_worker(video_file1, audio_file, offer_sdr, answer_rcv).await {
            println!("[Speaker] Error: {:?}", e);
        }
    });
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
    peer_connection
        .add_transceiver_from_kind(
            RTPCodecType::Audio,
            Some(RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Sendrecv,
                send_encodings: vec![],
            }),
        )
        .await?;
    peer_connection
        .add_transceiver_from_kind(
            RTPCodecType::Video,
            Some(RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Sendrecv,
                send_encodings: vec![],
            }),
        )
        .await?;
    let pc1 = Arc::downgrade(&peer_connection);
    let close_notify = Arc::new(Notify::new());
    let notify1 = close_notify.clone();
    peer_connection.on_track(Box::new(
        move |track: Arc<TrackRemote>,
              _receiver: Arc<RTCRtpReceiver>,
              _tranceiver: Arc<RTCRtpTransceiver>| {
            let media_ssrc = track.ssrc();
            let pc2 = pc1.clone();
            let kind = track.kind();
            let notify2 = notify1.clone();
            println!("[Listener] track codec {:?}", track.codec());
            if kind == RTPCodecType::Video {
                tokio::spawn(async move {
                    let mut ticker = tokio::time::interval(Duration::from_secs(2));
                    while let Some(pc3) = pc2.upgrade() {
                        if peer_closed(&pc3) {
                            break;
                        }
                        if pc3
                            .write_rtcp(&[Box::new(PictureLossIndication {
                                sender_ssrc: 0,
                                media_ssrc,
                            })])
                            .await
                            .is_err()
                        {
                            break;
                        }
                        let _ = ticker.tick().await;
                    }
                    println!("[Listener] closing {kind} pli thread");
                });
            }

            let pc2 = pc1.clone();
            let video_file1 = video_file.clone();
            match kind {
                RTPCodecType::Video => {
                    tokio::spawn(async move {
                        let mut pck = H265Packet::default();
                        let mut fdata = BytesMut::new();
                        loop {
                            let timeout = tokio::time::sleep(Duration::from_secs(4));
                            tokio::pin!(timeout);
                            tokio::select! {
                                _ = timeout.as_mut() => {
                                    break;
                                }
                                m = track.read_rtp() => {
                                    println!("rtp readed");
                                    if let Ok((p, _)) = m {
                                        let data = pck.depacketize(&p.payload).unwrap();
                                        match pck.payload() {
                                            H265Payload::H265PACIPacket(p) => {
                                                println!("[Listener] paci {:?}", p.payload_header());
                                            }
                                            H265Payload::H265SingleNALUnitPacket(p) => {
                                                println!(
                                                    "[Listener] single len {:?} type {:?}",
                                                    p.payload().len(),
                                                    p.payload_header().nalu_type()
                                                );
                                                fdata.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
                                                fdata.extend_from_slice(&data);
                                            }
                                            H265Payload::H265AggregationPacket(p) => {
                                                if let Some(uf) = p.first_unit() {
                                                    println!(
                                                        "[Listener] aggr first nal len {} type {:?}",
                                                        uf.nal_unit().len(),
                                                        UnitType::for_id((uf.nal_unit()[0] & 0b0111_1110) >> 1)
                                                    );
                                                    fdata.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
                                                    fdata.extend_from_slice(&uf.nal_unit());
                                                }
                                                for ou in p.other_units() {
                                                    println!(
                                                        "[Listener] aggr other nal len {} type {:?}",
                                                        ou.nal_unit().len(),
                                                        UnitType::for_id((ou.nal_unit()[0] & 0b0111_1110) >> 1)
                                                    );
                                                    fdata.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
                                                    fdata.extend_from_slice(&ou.nal_unit());
                                                }
                                            }
                                            H265Payload::H265FragmentationUnitPacket(p) => {
                                                println!(
                                                    "[Listener] fu nal header {:?} data4 {:?}, nal_type {:?}",
                                                    p.fu_header(),
                                                    &data[0..4],
                                                    p.fu_header().fu_type(),
                                                );
                                                if p.fu_header().s() {
                                                    let nal_type = (p.fu_header().fu_type() << 1) & 0b0111_1110;
                                                    fdata.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
                                                    fdata.extend_from_slice(&[nal_type, 0x01]);
                                                }
                                                fdata.extend_from_slice(&p.payload());
                                                if p.fu_header().e() {
                                                    println!("[Listener] fu nal collected");
                                                }
                                            }
                                        }
                                    } else if weak_peer_closed(&pc2) {
                                        println!("peer abnormally closed");
                                        break;
                                    }
                                }
                            }
                        }
                        let mut file = std::fs::File::create(format!("{video_file1}.output")).unwrap();
                        let _ = file.write_all(&fdata);
                        println!("[Listener] closing video read thread");
                        notify2.notify_waiters();
                    });
                }
                RTPCodecType::Audio => {
                    tokio::spawn(async move {
                        loop {
                            let timeout = tokio::time::sleep(Duration::from_secs(4));
                            tokio::pin!(timeout);
                            tokio::select! {
                                _ = timeout.as_mut() => {
                                    break;
                                }
                                m = track.read_rtp() => {
                                    if m.is_err() && weak_peer_closed(&pc2) {
                                        break;
                                    }
                                }
                            }
                        }
                        println!("[Listener] closing audio read thread");
                        notify2.notify_waiters();
                    });
                }
                _ => {}
            }
            Box::pin(async {})
        },
    ));
    let notify1 = close_notify.clone();
    peer_connection.on_ice_connection_state_change(Box::new(
        move |connection_state: RTCIceConnectionState| {
            println!("[Listener] session state changed {connection_state}",);
            if connection_state == RTCIceConnectionState::Closed
                || connection_state == RTCIceConnectionState::Failed
            {
                notify1.notify_waiters();
            }
            Box::pin(async {})
        },
    ));

    println!("[Listener] waiting for offer");
    let timeout = tokio::time::sleep(Duration::from_secs(60));
    tokio::pin!(timeout);
    let offer = tokio::select! {
        _ = timeout.as_mut() => {panic!("wait offer failed")}
        sdp = offer_rcv.recv() => {sdp.unwrap()}
    };
    peer_connection.set_remote_description(offer).await?;
    let answer = peer_connection.create_answer(None).await?;
    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    peer_connection.set_local_description(answer).await?;
    let _ = gather_complete.recv().await;

    println!("[Listener] offer set, sending answer");
    if let Some(answer) = peer_connection.local_description().await {
        let _ = answer_sdr.send(answer).await;
    }

    println!("[Listener] answer sent, await quit event");
    let timeout = tokio::time::sleep(Duration::from_secs(60));
    tokio::pin!(timeout);
    tokio::select! {
        _ = timeout.as_mut() => {}
        _ = close_notify.notified() => {}
    }
    let _ = peer_connection.close().await;
    println!("[Listener] closing peer");

    Ok(())
}

async fn offer_worker(
    video_file: String,
    audio_file: String,
    offer_sdr: mpsc::Sender<RTCSessionDescription>,
    mut answer_rcv: mpsc::Receiver<RTCSessionDescription>,
) -> Result<()> {
    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);
    let video_done_tx = done_tx.clone();
    let audio_done_tx = done_tx.clone();

    // Prepare the configuration
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Create a MediaEngine object to configure the supported codec
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;

    let mut registry = Registry::new();

    // Use the default set of Interceptors
    registry = register_default_interceptors(registry, &mut m)?;

    // Create the API object with the MediaEngine
    let api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();

    // Create a new RTCPeerConnection
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);
    let notify_connect = Arc::new(Notify::new());

    let local_video_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_HEVC.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    let video_rtp_sender = peer_connection
        .add_track(Arc::clone(&local_video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;
    tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = video_rtp_sender.read(&mut rtcp_buf).await {}
        Result::<()>::Ok(())
    });
    let notify1 = notify_connect.clone();
    tokio::spawn(async move {
        let mut buf = vec![];
        let mut file = File::open(&video_file).unwrap();
        let _ = file.read_to_end(&mut buf);
        let mut data = BytesMut::from_iter(buf);

        let list = memchr::memmem::find_iter(&data, &ANNEXB_NALUSTART_CODE);
        let mut data_list = vec![];
        let mut idxs = list.into_iter().collect::<Vec<usize>>();
        idxs.reverse();
        for i in idxs {
            let nal_data = data.split_off(i);
            // let payload_header = H265NALUHeader::new(nal_data[4], nal_data[5]);
            // let payload_nalu_type = payload_header.nalu_type();
            // let nalu_type = UnitType::for_id(payload_nalu_type).unwrap_or(UnitType::IGNORE);
            data_list.insert(0, nal_data);
        }

        let timeout = tokio::time::sleep(Duration::from_secs(10));
        tokio::pin!(timeout);
        tokio::select! {
            _ = timeout.as_mut() => {return;}
            _ = notify1.notified()=> {}
        };
        println!("[Speaker] play video from disk file");
        let mut ticker = tokio::time::interval(Duration::from_millis(33));
        loop {
            if data_list.is_empty() {
                break;
            }
            let nal_data = data_list.remove(0);
            let payload_header = H265NALUHeader::new(nal_data[4], nal_data[5]);
            let payload_nalu_type = payload_header.nalu_type();
            let nalu_type = UnitType::for_id(payload_nalu_type).unwrap_or(UnitType::IGNORE);
            if let Err(e) = local_video_track
                .write_sample(&Sample {
                    data: nal_data.freeze(),
                    duration: Duration::from_secs(1),
                    ..Default::default()
                })
                .await
            {
                println!("[Speaker] sending video err {e}");
            }

            if nalu_type != UnitType::VPS
                || nalu_type != UnitType::SPS
                || nalu_type != UnitType::PPS
                || nalu_type != UnitType::SEI
            {
                let _ = ticker.tick().await;
            }
        }
        let _ = video_done_tx.try_send(());
    });

    let local_audio_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            ..Default::default()
        },
        "audio".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    let audio_rtp_sender = peer_connection
        .add_track(Arc::clone(&local_audio_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;
    tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = audio_rtp_sender.read(&mut rtcp_buf).await {}
        Result::<()>::Ok(())
    });
    let notify1 = notify_connect.clone();
    tokio::spawn(async move {
        // Open a IVF file and start reading using our IVFReader
        let file = File::open(&audio_file)?;
        let reader = BufReader::new(file);
        // Open on oggfile in non-checksum mode.
        let (mut ogg, _) = OggReader::new(reader, true)?;
        // Wait for connection established
        notify1.notified().await;
        println!("[Speaker] play audio from disk file output.ogg");
        // It is important to use a time.Ticker instead of time.Sleep because
        // * avoids accumulating skew, just calling time.Sleep didn't compensate for the time spent parsing the data
        // * works around latency issues with Sleep
        let mut ticker = tokio::time::interval(OGG_PAGE_DURATION);
        // Keep track of last granule, the difference is the amount of samples in the buffer
        let mut last_granule: u64 = 0;
        while let Ok((page_data, page_header)) = ogg.parse_next_page() {
            // The amount of samples is the difference between the last and current timestamp
            let sample_count = page_header.granule_position - last_granule;
            last_granule = page_header.granule_position;
            let sample_duration = Duration::from_millis(sample_count * 1000 / 48000);
            if let Err(e) = local_audio_track
                .write_sample(&Sample {
                    data: page_data.freeze(),
                    duration: sample_duration,
                    ..Default::default()
                })
                .await
            {
                println!("[Speaker] sending audio err {e}");
            }
            let _ = ticker.tick().await;
        }
        let _ = audio_done_tx.try_send(());
        Result::<()>::Ok(())
    });

    let notify1 = notify_connect.clone();
    peer_connection.on_ice_connection_state_change(Box::new(
        move |connection_state: RTCIceConnectionState| {
            println!("[Speaker] session state changed {connection_state}",);
            if connection_state == RTCIceConnectionState::Connected {
                notify1.notify_waiters();
            }
            Box::pin(async {})
        },
    ));
    // let pc = Arc::downgrade(&peer_connection);
    // let mut candidates = Arc::new(Mutex::new(vec![]));
    // let candidates1 = candidates.clone();
    // let notify_gather = Arc::new(Notify::new());
    // let notify1 = notify_gather.clone();
    // peer_connection.on_ice_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
    //     let pc2 = pc.clone();
    //     let pending_candidates3 = Arc::clone(&pending_candidates2);
    //     Box::pin(async move {
    //         if let Some(c) = c {
    //             candidates1.lock().await.push(c);
    //         } else {
    //             notify1.notify_waiters();
    //         }
    //     })
    // }));
    let offer = peer_connection.create_offer(None).await?;
    // Create channel that is blocked until ICE Gathering is complete
    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    // Sets the LocalDescription, and starts our UDP listeners
    peer_connection.set_local_description(offer).await?;
    let _ = gather_complete.recv().await;

    if let Some(sdp) = peer_connection.local_description().await {
        let _ = offer_sdr.send(sdp).await;
    }
    println!("[Speaker] offer sent, waiting for answer");
    let answer = answer_rcv.recv().await.unwrap();
    peer_connection.set_remote_description(answer).await?;
    println!("[Speaker] answer received, wait for quit event");

    let timeout = tokio::time::sleep(Duration::from_secs(30));
    tokio::pin!(timeout);
    tokio::select! {
        _ = timeout.as_mut() => {}
        _ = done_rx.recv() => {}
    }
    peer_connection.close().await?;
    println!("[Speaker] closing peer");
    Ok(())
}

pub fn peer_closed(conn: &Arc<RTCPeerConnection>) -> bool {
    let state = conn.connection_state();
    state == RTCPeerConnectionState::Closed || state == RTCPeerConnectionState::Failed
}

pub fn weak_peer_closed(conn: &Weak<RTCPeerConnection>) -> bool {
    let mut result = false;
    if let Some(pc3) = conn.upgrade() {
        if peer_closed(&pc3) {
            result = true;
        }
    } else {
        result = true
    }
    result
}

// #[derive(Clone, Debug)]
// pub struct Nal {
//     pub type_: UnitType,
//     pub data: Vec<u8>,
// }

// impl Nal {
//     pub fn new(data: Vec<u8>) -> Result<Self> {
//         Ok(Self {
//             type_: Self::nal_unit_type(&data)?,
//             data,
//         })
//     }
//     pub fn nal_unit_type(data: &[u8]) -> Result<UnitType> {
//         UnitType::for_id((data[0] & 0b0111_1110) >> 1)
//     }
// }
