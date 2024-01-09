use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::{RTCDataChannel, RTCDataChannelEventHandler};
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::{PeerConnectionEventHandler, RTCPeerConnection};

const BUFFERED_AMOUNT_LOW_THRESHOLD: usize = 512 * 1024; // 512 KB
const MAX_BUFFERED_AMOUNT: usize = 1024 * 1024; // 1 MB

async fn create_peer_connection() -> anyhow::Result<RTCPeerConnection> {
    // Create unique MediaEngine,
    // as MediaEngine must not be shared between PeerConnections
    let mut media_engine = MediaEngine::default();

    media_engine.register_default_codecs()?;

    let mut interceptor_registry = Registry::new();

    interceptor_registry = register_default_interceptors(interceptor_registry, &mut media_engine)?;

    // Create API that bundles the global functions of the WebRTC API
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
    // Create a peer connection first
    let pc = create_peer_connection().await?;

    // Data transmission requires a data channel, so prepare to create one
    let options = Some(RTCDataChannelInit {
        ordered: Some(false),
        max_retransmits: Some(0u16),
        ..Default::default()
    });

    // Create a data channel to send data over a peer connection
    let dc = pc.create_data_channel("data", options).await?;

    // Use mpsc channel to send and receive a signal when more data can be sent
    let (more_can_be_sent, maybe_more_can_be_sent) = tokio::sync::mpsc::channel::<()>(1);

    struct ChannelHandler {
        data_channel: Arc<RTCDataChannel>,
        maybe_more_can_be_sent: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<()>>>,
        more_can_be_sent: tokio::sync::mpsc::Sender<()>,
    }

    impl RTCDataChannelEventHandler for ChannelHandler {
        fn on_open(&mut self) -> impl Future<Output = ()> + Send {
            // This callback shouldn't be blocked for a long time, so we spawn our handler

            let maybe_more_can_be_sent = self.maybe_more_can_be_sent.clone();
            let data_channel = self.data_channel.clone();
            tokio::spawn(async move {
                let buf = Bytes::from_static(&[0u8; 1024]);

                loop {
                    if data_channel.send(&buf).await.is_err() {
                        break;
                    }

                    let buffered_amount = data_channel.buffered_amount().await;

                    if buffered_amount + buf.len() > MAX_BUFFERED_AMOUNT {
                        // Wait for the signal that more can be sent
                        let mut wait_for_more = maybe_more_can_be_sent.lock().await;
                        let _ = wait_for_more.recv().await;
                    }
                }
            });
            async {}
        }

        fn on_buffered_amount_low(&mut self, _: ()) -> impl Future<Output = ()> + Send {
            async move {
                // Send a signal that more can be sent
                self.more_can_be_sent.send(()).await.unwrap();
            }
        }
    }
    dc.with_event_handler(ChannelHandler {
        data_channel: dc.clone(),
        more_can_be_sent,
        maybe_more_can_be_sent: Arc::new(maybe_more_can_be_sent),
    });

    dc.set_buffered_amount_low_threshold(BUFFERED_AMOUNT_LOW_THRESHOLD)
        .await;

    Ok(pc)
}

async fn create_responder() -> anyhow::Result<RTCPeerConnection> {
    // Create a peer connection first
    let pc = create_peer_connection().await?;

    // Set a data channel handler so that we can receive data
    pc.on_data_channel(Box::new(move |dc| {
        Box::pin(async move {
            let total_bytes_received = Arc::new(AtomicUsize::new(0));

            let shared_total_bytes_received = total_bytes_received.clone();
            dc.on_open(Box::new(move || {
                Box::pin(async {
                    // This callback shouldn't be blocked for a long time, so we spawn our handler
                    tokio::spawn(async move {
                        let start = SystemTime::now();

                        tokio::time::sleep(Duration::from_secs(1)).await;
                        println!();

                        loop {
                            let total_bytes_received =
                                shared_total_bytes_received.load(Ordering::Relaxed);

                            let elapsed = SystemTime::now().duration_since(start);
                            let bps =
                                (total_bytes_received * 8) as f64 / elapsed.unwrap().as_secs_f64();

                            println!(
                                "Throughput is about {:.03} Mbps",
                                bps / (1024 * 1024) as f64
                            );
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    });
                })
            }));

            dc.on_message(Box::new(move |msg| {
                let total_bytes_received = total_bytes_received.clone();

                Box::pin(async move {
                    total_bytes_received.fetch_add(msg.data.len(), Ordering::Relaxed);
                })
            }));
        })
    }));

    Ok(pc)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let requester = Arc::new(create_requester().await?);
    let responder = Arc::new(create_responder().await?);

    struct ResponderHandler {
        maybe_responder: std::sync::Weak<RTCPeerConnection>,
        fault: tokio::sync::mpsc::Sender<()>,
    }

    impl PeerConnectionEventHandler for ResponderHandler {
        fn on_ice_candidate(
            &mut self,
            candidate: Option<RTCIceCandidate>,
        ) -> impl Future<Output = ()> + Send {
            async move {
                if let (Some(requester), Some(Ok(candidate))) = (
                    self.maybe_responder.upgrade(),
                    candidate.map(|c| c.to_json()),
                ) {
                    if let Err(err) = requester.add_ice_candidate(candidate).await {
                        log::warn!("{err}");
                    }
                }
            }
        }
        fn on_peer_connection_state_change(
            &mut self,
            state: RTCPeerConnectionState,
        ) -> impl Future<Output = ()> + Send {
            async move {
                if state == RTCPeerConnectionState::Failed {
                    self.fault.send(()).await.unwrap();
                }
            }
        }

        fn on_data_channel(
            &mut self,
            channel: Arc<RTCDataChannel>,
        ) -> impl Future<Output = ()> + Send {
            struct ChannelHandler {
                bytes_received: Arc<AtomicUsize>,
            }

            impl RTCDataChannelEventHandler for ChannelHandler {
                fn on_open(&mut self) -> impl Future<Output = ()> + Send {
                    let bytes_received = self.bytes_received.clone();
                    tokio::spawn(async move {
                        let start = SystemTime::now();

                        tokio::time::sleep(Duration::from_secs(1)).await;
                        println!();

                        loop {
                            let total_bytes_received = bytes_received.load(Ordering::Relaxed);

                            let elapsed = SystemTime::now().duration_since(start);
                            let bps =
                                (total_bytes_received * 8) as f64 / elapsed.unwrap().as_secs_f64();

                            println!(
                                "Throughput is about {:.03} Mbps",
                                bps / (1024 * 1024) as f64
                            );
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    });
                    async {}
                }
                fn on_message(
                    &mut self,
                    msg: DataChannelMessage,
                ) -> impl Future<Output = ()> + Send {
                    self.bytes_received
                        .fetch_add(msg.data.len(), Ordering::Relaxed);
                    async {}
                }
            }

            channel.with_event_handler(ChannelHandler {
                bytes_received: Arc::new(AtomicUsize::new(0)),
            });
            async {}
        }
    }

    struct RequesterHandler {
        maybe_requester: std::sync::Weak<RTCPeerConnection>,
        fault: tokio::sync::mpsc::Sender<()>,
    }

    impl PeerConnectionEventHandler for RequesterHandler {
        fn on_ice_candidate(
            &mut self,
            candidate: Option<RTCIceCandidate>,
        ) -> impl Future<Output = ()> + Send {
            async move {
                if let (Some(requester), Some(Ok(candidate))) = (
                    self.maybe_requester.upgrade(),
                    candidate.map(|c| c.to_json()),
                ) {
                    if let Err(err) = requester.add_ice_candidate(candidate).await {
                        log::warn!("{err}");
                    }
                }
            }
        }
        fn on_peer_connection_state_change(
            &mut self,
            state: RTCPeerConnectionState,
        ) -> impl Future<Output = ()> + Send {
            async move {
                if state == RTCPeerConnectionState::Failed {
                    self.fault.send(()).await.unwrap();
                }
            }
        }
    }

    let maybe_requester = Arc::downgrade(&requester);
    let (fault, mut reqs_fault) = tokio::sync::mpsc::channel(1);
    requester.with_event_handler(RequesterHandler {
        maybe_requester,
        fault,
    });

    let maybe_responder = Arc::downgrade(&responder);
    let (fault, mut resp_fault) = tokio::sync::mpsc::channel(1);
    responder.with_event_handler(ResponderHandler {
        maybe_responder,
        fault,
    });

    let reqs = requester.create_offer(None).await?;

    requester.set_local_description(reqs.clone()).await?;
    responder.set_remote_description(reqs).await?;

    let resp = responder.create_answer(None).await?;

    responder.set_local_description(resp.clone()).await?;
    requester.set_remote_description(resp).await?;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = reqs_fault.recv() => {
            log::error!("Requester's peer connection failed...")
        }
        _ = resp_fault.recv() => {
            log::error!("Responder's peer connection failed...");
        }
    }

    requester.close().await?;
    responder.close().await?;

    println!();

    Ok(())
}
