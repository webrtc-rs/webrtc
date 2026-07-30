//! `on_data_channel` must fire only for channels the **peer** opened.
//!
//! The driver announced every `OnOpen` through the handler, including channels this side
//! created itself. Two things went wrong with that:
//!
//!   1. An application that opens a channel and also implements `on_data_channel` sees its
//!      own channel come back as if the peer had opened it. Anything that treats the
//!      callback as "here is an incoming stream" (a muxer, a router) acts on a stream the
//!      peer knows nothing about.
//!   2. The announced handle is unusable anyway. `create_data_channel` already registered
//!      the event sender for that id, so the driver's `Entry::Vacant` check declined to
//!      replace it — the `DataChannelImpl` handed to the callback carried a fresh receiver
//!      whose sender was dropped on the spot. It never yields a single event.
//!
//! Only the offerer opens a channel here. The answerer must see exactly it; the offerer must
//! see nothing at all. Before the fix the offerer was handed its own channel back.
//!
//! (Deliberately one-sided: having both peers open an in-band channel *before* the DTLS role
//! is settled makes `generate_data_channel_id` hand out the same id on both sides, which is a
//! separate problem and would muddy this assertion.)

use anyhow::Result;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Sender, channel};

mod common;
use common::{block_on, runtime, sleep, timeout};

/// Records the label of every channel announced through `on_data_channel`.
struct Handler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
    announced: Arc<Mutex<Vec<String>>>,
    announced_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let label = dc.label().await.unwrap_or_default();
        self.announced.lock().unwrap().push(label);
        let _ = self.announced_tx.try_send(());
    }
}

struct Peer {
    pc: Box<dyn PeerConnection>,
    announced: Arc<Mutex<Vec<String>>>,
    gather_rx: webrtc::runtime::Receiver<()>,
    connected_rx: webrtc::runtime::Receiver<()>,
    announced_rx: webrtc::runtime::Receiver<()>,
}

async fn build_peer(runtime: Arc<dyn webrtc::runtime::Runtime>) -> Result<Peer> {
    let (gather_tx, gather_rx) = channel::<()>(1);
    let (connected_tx, connected_rx) = channel::<()>(1);
    // Sized above the expected count so a regression queues its extra announcement rather
    // than dropping it — the assertion should fail loudly, not hang.
    let (announced_tx, announced_rx) = channel::<()>(8);
    let announced = Arc::new(Mutex::new(Vec::new()));

    let pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(Handler {
            gather_tx,
            connected_tx,
            announced: announced.clone(),
            announced_tx,
        }))
        .with_runtime(runtime)
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    Ok(Peer {
        pc: Box::new(pc),
        announced,
        gather_rx,
        connected_rx,
        announced_rx,
    })
}

#[test]
fn test_on_data_channel_fires_only_for_peer_opened_channels() {
    block_on(run()).unwrap();
}

async fn run() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let runtime = runtime();

    let mut offerer = build_peer(runtime.clone()).await?;
    let mut answerer = build_peer(runtime.clone()).await?;

    // Only the offerer opens a channel.
    let offerer_dc = offerer
        .pc
        .create_data_channel("from-offerer", Some(RTCDataChannelInit::default()))
        .await?;

    // Offer / answer over loopback.
    let offer = offerer.pc.create_offer(None).await?;
    offerer.pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), offerer.gather_rx.recv()).await;
    let offer_sdp = offerer
        .pc
        .local_description()
        .await
        .expect("offerer local description");

    answerer.pc.set_remote_description(offer_sdp).await?;
    let answer = answerer.pc.create_answer(None).await?;
    answerer.pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), answerer.gather_rx.recv()).await;
    let answer_sdp = answerer
        .pc
        .local_description()
        .await
        .expect("answerer local description");
    offerer.pc.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), offerer.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: offerer connect"))?;
    timeout(Duration::from_secs(5), answerer.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: answerer connect"))?;

    // The answerer must be told about it.
    timeout(Duration::from_secs(10), answerer.announced_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: answerer never saw the peer's channel"))?;

    // Give the regression a chance to deliver the surplus announcement before asserting —
    // the offerer's own channel is open well before this point.
    sleep(Duration::from_millis(500)).await;

    assert_eq!(
        *answerer.announced.lock().unwrap(),
        vec!["from-offerer".to_string()],
        "answerer must be told about the channel the offerer opened"
    );
    assert!(
        offerer.announced.lock().unwrap().is_empty(),
        "the offerer opened that channel itself; on_data_channel must not report it back \
         (saw {:?})",
        offerer.announced.lock().unwrap()
    );

    // The locally created handle keeps working — the fix must not disturb its event stream,
    // which is exactly what the surplus announcement used to steal a receiver for.
    assert_eq!(
        offerer_dc.ready_state().await?,
        webrtc::data_channel::RTCDataChannelState::Open,
        "the offerer's own channel should be open"
    );

    Ok(())
}

/// A `negotiated` channel is opened out of band on both sides, so neither peer should ever
/// hear about it through `on_data_channel` (W3C `RTCDataChannelInit.negotiated`).
///
/// This is the shape libp2p's WebRTC transports use for their Noise handshake stream: the
/// surplus announcement handed that channel to the muxer as if it were an incoming stream.
#[test]
fn test_negotiated_channel_is_never_announced() {
    block_on(run_negotiated()).unwrap();
}

async fn run_negotiated() -> Result<()> {
    let runtime = runtime();

    let mut offerer = build_peer(runtime.clone()).await?;
    let mut answerer = build_peer(runtime.clone()).await?;

    let negotiated = || {
        Some(RTCDataChannelInit {
            negotiated: Some(0),
            ordered: true,
            ..Default::default()
        })
    };
    let offerer_dc = offerer.pc.create_data_channel("", negotiated()).await?;
    let _answerer_dc = answerer.pc.create_data_channel("", negotiated()).await?;

    let offer = offerer.pc.create_offer(None).await?;
    offerer.pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), offerer.gather_rx.recv()).await;
    let offer_sdp = offerer.pc.local_description().await.expect("offer");

    answerer.pc.set_remote_description(offer_sdp).await?;
    let answer = answerer.pc.create_answer(None).await?;
    answerer.pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), answerer.gather_rx.recv()).await;
    let answer_sdp = answerer.pc.local_description().await.expect("answer");
    offerer.pc.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), offerer.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: offerer connect"))?;
    timeout(Duration::from_secs(5), answerer.connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: answerer connect"))?;

    // Wait for the channel to actually open, otherwise "nothing was announced" would pass
    // trivially.
    let (open_tx, mut open_rx) = channel::<()>(1);
    {
        let dc = offerer_dc.clone();
        runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        let _ = open_tx.try_send(());
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }
    timeout(Duration::from_secs(10), open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: negotiated channel never opened"))?;
    sleep(Duration::from_millis(500)).await;

    assert!(
        offerer.announced.lock().unwrap().is_empty(),
        "a negotiated channel must never reach on_data_channel (offerer saw {:?})",
        offerer.announced.lock().unwrap()
    );
    assert!(
        answerer.announced.lock().unwrap().is_empty(),
        "a negotiated channel must never reach on_data_channel (answerer saw {:?})",
        answerer.announced.lock().unwrap()
    );

    Ok(())
}
