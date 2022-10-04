use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};

use bytes::Bytes;

use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors, media_engine::MediaEngine, APIBuilder,
    },
    data_channel::data_channel_init::RTCDataChannelInit,
    ice_transport::{ice_candidate::RTCIceCandidate, ice_server::RTCIceServer},
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        RTCPeerConnection,
    },
};

const BUFFERED_AMOUNT_LOW_THRESHOLD: usize = 512 * 1024; // 512 KB
const MAX_BUFFERED_AMOUNT: usize = 1024 * 1024; // 1 MB

async fn create_peer_connection() -> anyhow::Result<RTCPeerConnection> {
    let mut media_engine = MediaEngine::default();

    media_engine.register_default_codecs()?;

    let mut interceptor_registry = Registry::new();

    interceptor_registry = register_default_interceptors(interceptor_registry, &mut media_engine)?;

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(interceptor_registry)
        .build();

    let ice_servers = vec![RTCIceServer {
        ..Default::default()
    }];

    let config = RTCConfiguration {
        ice_servers,
        ..Default::default()
    };

    Ok(api.new_peer_connection(config).await?)
}

async fn create_requester() -> anyhow::Result<RTCPeerConnection> {
    let pc = create_peer_connection().await?;

    let options = Some(RTCDataChannelInit {
        ordered: Some(false),
        max_retransmits: Some(0u16),
        ..Default::default()
    });

    let (more_can_be_sent, mut maybe_more_can_be_sent) = tokio::sync::mpsc::channel(1);
    let dc = pc.create_data_channel("data", options).await?;

    let shared_dc = dc.clone();
    dc.on_open(Box::new(|| {
        Box::pin(async move {
            println!("requester :: on_open");

            tokio::spawn(async move {
                let buf = Bytes::from_static(&[0u8; 1024]);

                loop {
                    if shared_dc.send(&buf).await.is_err() {
                        break;
                    }

                    let buffered_amount = shared_dc.buffered_amount().await;

                    if buffered_amount + buf.len() > MAX_BUFFERED_AMOUNT {
                        let _ = maybe_more_can_be_sent.recv().await;
                    }
                }
            });
        })
    }))
    .await;

    dc.set_buffered_amount_low_threshold(BUFFERED_AMOUNT_LOW_THRESHOLD)
        .await;

    dc.on_buffered_amount_low(Box::new(move || {
        let more_can_be_sent = more_can_be_sent.clone();

        Box::pin(async move {
            more_can_be_sent.send(()).await.unwrap();
        })
    }))
    .await;

    Ok(pc)
}

async fn create_responder() -> anyhow::Result<RTCPeerConnection> {
    let pc = create_peer_connection().await?;

    pc.on_data_channel(Box::new(move |dc| {
        Box::pin(async move {
            let total_bytes_received = Arc::new(AtomicUsize::new(0));

            let get_total_bytes_received = total_bytes_received.clone();
            dc.on_open(Box::new(move || {
                Box::pin(async {
                    println!("responder :: on_open");

                    tokio::spawn(async move {
                        let start = SystemTime::now();

                        tokio::time::sleep(Duration::from_secs(1)).await;
                        println!("");

                        loop {
                            let total_bytes = get_total_bytes_received.load(Ordering::Relaxed);

                            let elapsed = SystemTime::now().duration_since(start);
                            let bps = (total_bytes * 8) as f64 / elapsed.unwrap().as_secs_f64();

                            println!(
                                "Throughput is about {:.03} Mbps",
                                bps / (1024 * 1024) as f64
                            );
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    });
                })
            }))
            .await;

            dc.on_message(Box::new(move |msg| {
                let total_bytes_received = total_bytes_received.clone();

                Box::pin(async move {
                    total_bytes_received.fetch_add(msg.data.len(), Ordering::Relaxed);
                })
            }))
            .await;
        })
    }))
    .await;

    Ok(pc)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let requester = Arc::new(create_requester().await?);
    let responder = Arc::new(create_responder().await?);

    let maybe_requester = Arc::downgrade(&requester);
    responder
        .on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            let maybe_requester = maybe_requester.clone();

            Box::pin(async move {
                if let Some(candidate) = candidate {
                    if let Ok(candidate) = candidate.to_json().await {
                        if let Some(requester) = maybe_requester.upgrade() {
                            requester.add_ice_candidate(candidate).await.unwrap();
                        }
                    }
                }
            })
        }))
        .await;

    let maybe_responder = Arc::downgrade(&responder);
    requester
        .on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            let maybe_responder = maybe_responder.clone();

            Box::pin(async move {
                if let Some(candidate) = candidate {
                    if let Ok(candidate) = candidate.to_json().await {
                        if let Some(responder) = maybe_responder.upgrade() {
                            responder.add_ice_candidate(candidate).await.unwrap();
                        }
                    }
                }
            })
        }))
        .await;

    let (fault, mut reqs_fault) = tokio::sync::mpsc::channel(1);
    requester
        .on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            let fault = fault.clone();

            Box::pin(async move {
                println!("requester :: peer_connection_state_change :: {}", s);

                if s == RTCPeerConnectionState::Failed {
                    println!("{:?}", s);

                    let _ = fault.try_send(());
                }
            })
        }))
        .await;

    let (fault, mut resp_fault) = tokio::sync::mpsc::channel(1);
    responder
        .on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            let fault = fault.clone();

            Box::pin(async move {
                println!("responder :: peer_connection_state_change :: {}", s);

                if s == RTCPeerConnectionState::Failed {
                    println!("{:?}", s);

                    let _ = fault.try_send(());
                }
            })
        }))
        .await;

    let reqs = requester.create_offer(None).await?;

    requester.set_local_description(reqs.clone()).await?;
    responder.set_remote_description(reqs).await?;

    let resp = responder.create_answer(None).await?;

    responder.set_local_description(resp.clone()).await?;
    requester.set_remote_description(resp).await?;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("");
        }
        _ = reqs_fault.recv() => {
            println!("reqs_fault");
        }
        _ = resp_fault.recv() => {
            println!("resp_fault");
        }
    }

    if let Err(err) = requester.close().await {
        println!("{}", err);
    }

    if let Err(err) = responder.close().await {
        println!("{}", err);
    }

    println!("");

    Ok(())
}
