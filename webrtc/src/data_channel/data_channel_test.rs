// Silence warning on `for i in 0..vec.len() { … }`:
#![allow(clippy::needless_range_loop)]

use regex::Regex;
use tokio::sync::mpsc;
use tokio::time::Duration;
use waitgroup::WaitGroup;

use super::*;
use crate::api::media_engine::MediaEngine;
use crate::api::{APIBuilder, API};
use crate::data_channel::data_channel_init::RTCDataChannelInit;
//use log::LevelFilter;
//use std::io::Write;
use crate::dtls_transport::dtls_parameters::DTLSParameters;
use crate::dtls_transport::RTCDtlsTransport;
use crate::error::flatten_errs;
use crate::ice_transport::ice_candidate::RTCIceCandidate;
use crate::ice_transport::ice_connection_state::RTCIceConnectionState;
use crate::ice_transport::ice_gatherer::{RTCIceGatherOptions, RTCIceGatherer};
use crate::ice_transport::ice_parameters::RTCIceParameters;
use crate::ice_transport::ice_role::RTCIceRole;
use crate::ice_transport::RTCIceTransport;
use crate::peer_connection::configuration::RTCConfiguration;
use crate::peer_connection::peer_connection_test::*;
use crate::peer_connection::RTCPeerConnection;
use crate::sctp_transport::sctp_transport_capabilities::SCTPTransportCapabilities;

// EXPECTED_LABEL represents the label of the data channel we are trying to test.
// Some other channels may have been created during initialization (in the Wasm
// bindings this is a requirement).
const EXPECTED_LABEL: &str = "data";

async fn set_up_data_channel_parameters_test(
    api: &API,
    options: Option<RTCDataChannelInit>,
) -> Result<(
    RTCPeerConnection,
    RTCPeerConnection,
    Arc<RTCDataChannel>,
    mpsc::Sender<()>,
    mpsc::Receiver<()>,
)> {
    let (offer_pc, answer_pc) = new_pair(api).await?;
    let (done_tx, done_rx) = mpsc::channel(1);

    let dc = offer_pc
        .create_data_channel(EXPECTED_LABEL, options)
        .await?;
    Ok((offer_pc, answer_pc, dc, done_tx, done_rx))
}

async fn close_reliability_param_test(
    pc1: &mut RTCPeerConnection,
    pc2: &mut RTCPeerConnection,
    done_rx: mpsc::Receiver<()>,
) -> Result<()> {
    signal_pair(pc1, pc2).await?;

    close_pair(pc1, pc2, done_rx).await;

    Ok(())
}

/*
TODO: #[tokio::test] async fnBenchmarkDataChannelSend2(b *testing.B)  { benchmarkDataChannelSend(b, 2) }
#[tokio::test] async fnBenchmarkDataChannelSend4(b *testing.B)  { benchmarkDataChannelSend(b, 4) }
#[tokio::test] async fnBenchmarkDataChannelSend8(b *testing.B)  { benchmarkDataChannelSend(b, 8) }
#[tokio::test] async fnBenchmarkDataChannelSend16(b *testing.B) { benchmarkDataChannelSend(b, 16) }
#[tokio::test] async fnBenchmarkDataChannelSend32(b *testing.B) { benchmarkDataChannelSend(b, 32) }

// See https://github.com/pion/webrtc/issues/1516
#[tokio::test] async fnbenchmarkDataChannelSend(b *testing.B, numChannels int) {
    offerPC, answerPC, err := newPair()
    if err != nil {
        b.Fatalf("Failed to create a PC pair for testing")
    }

    open := make(map[string]chan bool)
    answerPC.OnDataChannel(func(d *DataChannel) {
        if _, ok := open[d.Label()]; !ok {
            // Ignore anything unknown channel label.
            return
        }
        d.OnOpen(func() { open[d.Label()] <- true })
    })

    var wg sync.WaitGroup
    for i := 0; i < numChannels; i++ {
        label := fmt.Sprintf("dc-%d", i)
        open[label] = make(chan bool)
        wg.Add(1)
        dc, err := offerPC.CreateDataChannel(label, nil)
        assert.NoError(b, err)

        dc.OnOpen(func() {
            <-open[label]
            for n := 0; n < b.N/numChannels; n++ {
                if err := dc.SendText("Ping"); err != nil {
                    b.Fatalf("Unexpected error sending data (label=%q): %v", label, err)
                }
            }
            wg.Done()
        })
    }

    assert.NoError(b, signalPair(offerPC, answerPC))
    wg.Wait()
    close_pair_now(b, offerPC, answerPC)
}
*/

#[tokio::test]
async fn test_data_channel_open() -> Result<()> {
    /*env_logger::Builder::new()
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
    .filter(None, LevelFilter::Trace)
    .init();*/

    //"handler should be called once"
    {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        let api = APIBuilder::new().with_media_engine(m).build();

        let (mut offer_pc, mut answer_pc) = new_pair(&api).await?;

        let (done_tx, done_rx) = mpsc::channel(1);
        let (open_calls_tx, mut open_calls_rx) = mpsc::channel(2);

        let open_calls_tx = Arc::new(open_calls_tx);
        let done_tx = Arc::new(done_tx);
        answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
            if d.label() == EXPECTED_LABEL {
                let open_calls_tx2 = Arc::clone(&open_calls_tx);
                let done_tx2 = Arc::clone(&done_tx);
                Box::pin(async move {
                    d.on_open(Box::new(move || {
                        Box::pin(async move {
                            let _ = open_calls_tx2.send(()).await;
                        })
                    }));
                    d.on_message(Box::new(move |_: DataChannelMessage| {
                        let done_tx3 = Arc::clone(&done_tx2);
                        tokio::spawn(async move {
                            // Wait a little bit to ensure all messages are processed.
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            let _ = done_tx3.send(()).await;
                        });
                        Box::pin(async {})
                    }));
                })
            } else {
                Box::pin(async {})
            }
        }));

        let dc = offer_pc.create_data_channel(EXPECTED_LABEL, None).await?;

        let dc2 = Arc::clone(&dc);
        dc.on_open(Box::new(move || {
            Box::pin(async move {
                let result = dc2.send_text("Ping".to_owned()).await;
                assert!(result.is_ok(), "Failed to send string on data channel");
            })
        }));

        signal_pair(&mut offer_pc, &mut answer_pc).await?;

        close_pair(&offer_pc, &answer_pc, done_rx).await;

        let _ = open_calls_rx.recv().await;
    }

    Ok(())
}

#[tokio::test]
async fn test_data_channel_send_before_signaling() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    //"before signaling"

    let (mut offer_pc, mut answer_pc) = new_pair(&api).await?;

    answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        // Make sure this is the data channel we were looking for. (Not the one
        // created in signalPair).
        if d.label() != EXPECTED_LABEL {
            return Box::pin(async {});
        }
        Box::pin(async move {
            let d2 = Arc::clone(&d);
            d.on_message(Box::new(move |_: DataChannelMessage| {
                let d3 = Arc::clone(&d2);
                Box::pin(async move {
                    let result = d3.send(&Bytes::from(b"Pong".to_vec())).await;
                    assert!(result.is_ok(), "Failed to send string on data channel");
                })
            }));
            assert!(d.ordered(), "Ordered should be set to true");
        })
    }));

    let dc = offer_pc.create_data_channel(EXPECTED_LABEL, None).await?;

    assert!(dc.ordered(), "Ordered should be set to true");

    let dc2 = Arc::clone(&dc);
    dc.on_open(Box::new(move || {
        let dc3 = Arc::clone(&dc2);
        Box::pin(async move {
            let result = dc3.send_text("Ping".to_owned()).await;
            assert!(result.is_ok(), "Failed to send string on data channel");
        })
    }));

    let (done_tx, done_rx) = mpsc::channel::<()>(1);
    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    dc.on_message(Box::new(move |_: DataChannelMessage| {
        let done_tx2 = Arc::clone(&done_tx);
        Box::pin(async move {
            let mut done = done_tx2.lock().await;
            done.take();
        })
    }));

    signal_pair(&mut offer_pc, &mut answer_pc).await?;

    close_pair(&offer_pc, &answer_pc, done_rx).await;
    Ok(())
}

#[tokio::test]
async fn test_data_channel_send_after_connected() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut offer_pc, mut answer_pc) = new_pair(&api).await?;

    answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        // Make sure this is the data channel we were looking for. (Not the one
        // created in signalPair).
        if d.label() != EXPECTED_LABEL {
            return Box::pin(async {});
        }
        Box::pin(async move {
            let d2 = Arc::clone(&d);
            d.on_message(Box::new(move |_: DataChannelMessage| {
                let d3 = Arc::clone(&d2);

                Box::pin(async move {
                    let result = d3.send(&Bytes::from(b"Pong".to_vec())).await;
                    assert!(result.is_ok(), "Failed to send string on data channel");
                })
            }));
            assert!(d.ordered(), "Ordered should be set to true");
        })
    }));

    let dc = offer_pc
        .create_data_channel(EXPECTED_LABEL, None)
        .await
        .expect("Failed to create a PC pair for testing");

    let (done_tx, done_rx) = mpsc::channel::<()>(1);
    let done_tx = Arc::new(Mutex::new(Some(done_tx)));

    //once := &sync.Once{}
    offer_pc.on_ice_connection_state_change(Box::new(move |state: RTCIceConnectionState| {
        let done_tx1 = Arc::clone(&done_tx);
        let dc1 = Arc::clone(&dc);
        Box::pin(async move {
            if state == RTCIceConnectionState::Connected
                || state == RTCIceConnectionState::Completed
            {
                // wasm fires completed state multiple times
                /*once.Do(func()*/
                {
                    assert!(dc1.ordered(), "Ordered should be set to true");

                    dc1.on_message(Box::new(move |_: DataChannelMessage| {
                        let done_tx2 = Arc::clone(&done_tx1);
                        Box::pin(async move {
                            let mut done = done_tx2.lock().await;
                            done.take();
                        })
                    }));

                    if dc1.send_text("Ping".to_owned()).await.is_err() {
                        // wasm binding doesn't fire OnOpen (we probably already missed it)
                        let dc2 = Arc::clone(&dc1);
                        dc1.on_open(Box::new(move || {
                            let dc3 = Arc::clone(&dc2);
                            Box::pin(async move {
                                let result = dc3.send_text("Ping".to_owned()).await;
                                assert!(result.is_ok(), "Failed to send string on data channel");
                            })
                        }));
                    }
                }
            }
        })
    }));

    signal_pair(&mut offer_pc, &mut answer_pc).await?;

    close_pair(&offer_pc, &answer_pc, done_rx).await;

    Ok(())
}

#[tokio::test]
async fn test_data_channel_close() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    // "Close after PeerConnection Closed"
    {
        let (offer_pc, answer_pc) = new_pair(&api).await?;

        let dc = offer_pc.create_data_channel(EXPECTED_LABEL, None).await?;

        close_pair_now(&offer_pc, &answer_pc).await;
        dc.close().await?;
    }

    // "Close before connected"
    {
        let (offer_pc, answer_pc) = new_pair(&api).await?;

        let dc = offer_pc.create_data_channel(EXPECTED_LABEL, None).await?;

        dc.close().await?;
        close_pair_now(&offer_pc, &answer_pc).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_data_channel_parameters_max_packet_life_time_exchange() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let ordered = true;
    let max_packet_life_time = 3u16;
    let options = RTCDataChannelInit {
        ordered: Some(ordered),
        max_packet_life_time: Some(max_packet_life_time),
        ..Default::default()
    };

    let (mut offer_pc, mut answer_pc, dc, done_tx, done_rx) =
        set_up_data_channel_parameters_test(&api, Some(options)).await?;

    // Check if parameters are correctly set
    assert_eq!(
        dc.ordered(),
        ordered,
        "Ordered should be same value as set in DataChannelInit"
    );
    assert_eq!(
        dc.max_packet_lifetime(),
        Some(max_packet_life_time),
        "should match"
    );

    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        if d.label() != EXPECTED_LABEL {
            return Box::pin(async {});
        }
        // Check if parameters are correctly set
        assert_eq!(
            d.ordered(),
            ordered,
            "Ordered should be same value as set in DataChannelInit"
        );
        assert_eq!(
            d.max_packet_lifetime(),
            Some(max_packet_life_time),
            "should match"
        );
        let done_tx2 = Arc::clone(&done_tx);
        Box::pin(async move {
            let mut done = done_tx2.lock().await;
            done.take();
        })
    }));

    close_reliability_param_test(&mut offer_pc, &mut answer_pc, done_rx).await?;

    Ok(())
}

#[tokio::test]
async fn test_data_channel_parameters_max_retransmits_exchange() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let ordered = false;
    let max_retransmits = 3000u16;
    let options = RTCDataChannelInit {
        ordered: Some(ordered),
        max_retransmits: Some(max_retransmits),
        ..Default::default()
    };

    let (mut offer_pc, mut answer_pc, dc, done_tx, done_rx) =
        set_up_data_channel_parameters_test(&api, Some(options)).await?;

    // Check if parameters are correctly set
    assert!(!dc.ordered(), "Ordered should be set to false");
    assert_eq!(dc.max_retransmits(), Some(max_retransmits), "should match");

    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        // Make sure this is the data channel we were looking for. (Not the one
        // created in signalPair).
        if d.label() != EXPECTED_LABEL {
            return Box::pin(async {});
        }

        // Check if parameters are correctly set
        assert!(!d.ordered(), "Ordered should be set to false");
        assert_eq!(Some(max_retransmits), d.max_retransmits(), "should match");
        let done_tx2 = Arc::clone(&done_tx);
        Box::pin(async move {
            let mut done = done_tx2.lock().await;
            done.take();
        })
    }));

    close_reliability_param_test(&mut offer_pc, &mut answer_pc, done_rx).await?;

    Ok(())
}

#[tokio::test]
async fn test_data_channel_parameters_unreliable_unordered_exchange() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let ordered = false;
    let max_retransmits = Some(0);
    let max_packet_life_time = None;
    let options = RTCDataChannelInit {
        ordered: Some(ordered),
        max_retransmits,
        max_packet_life_time,
        ..Default::default()
    };

    let (mut offer_pc, mut answer_pc, dc, done_tx, done_rx) =
        set_up_data_channel_parameters_test(&api, Some(options)).await?;

    // Check if parameters are correctly set
    assert_eq!(
        dc.ordered(),
        ordered,
        "Ordered should be same value as set in DataChannelInit"
    );
    assert_eq!(dc.max_retransmits, max_retransmits, "should match");

    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        if d.label() != EXPECTED_LABEL {
            return Box::pin(async {});
        }
        // Check if parameters are correctly set
        assert_eq!(
            d.ordered(),
            ordered,
            "Ordered should be same value as set in DataChannelInit"
        );
        assert_eq!(d.max_retransmits(), max_retransmits, "should match");
        let done_tx2 = Arc::clone(&done_tx);
        Box::pin(async move {
            let mut done = done_tx2.lock().await;
            done.take();
        })
    }));

    close_reliability_param_test(&mut offer_pc, &mut answer_pc, done_rx).await?;

    Ok(())
}

#[tokio::test]
async fn test_data_channel_parameters_reliable_unordered_exchange() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let ordered = false;
    let max_retransmits = None;
    let max_packet_life_time = None;
    let options = RTCDataChannelInit {
        ordered: Some(ordered),
        max_retransmits,
        max_packet_life_time,
        ..Default::default()
    };

    let (mut offer_pc, mut answer_pc, dc, done_tx, done_rx) =
        set_up_data_channel_parameters_test(&api, Some(options)).await?;

    // Check if parameters are correctly set
    assert_eq!(
        dc.ordered(),
        ordered,
        "Ordered should be same value as set in DataChannelInit"
    );
    assert_eq!(dc.max_retransmits, max_retransmits, "should match");

    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        if d.label() != EXPECTED_LABEL {
            return Box::pin(async {});
        }
        // Check if parameters are correctly set
        assert_eq!(
            d.ordered(),
            ordered,
            "Ordered should be same value as set in DataChannelInit"
        );
        assert_eq!(d.max_retransmits(), max_retransmits, "should match");
        let done_tx2 = Arc::clone(&done_tx);
        Box::pin(async move {
            let mut done = done_tx2.lock().await;
            done.take();
        })
    }));

    close_reliability_param_test(&mut offer_pc, &mut answer_pc, done_rx).await?;

    Ok(())
}
#[tokio::test]
async fn test_data_channel_parameters_protocol_exchange() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let protocol = "json".to_owned();
    let options = RTCDataChannelInit {
        protocol: Some(protocol.clone()),
        ..Default::default()
    };

    let (mut offer_pc, mut answer_pc, dc, done_tx, done_rx) =
        set_up_data_channel_parameters_test(&api, Some(options)).await?;

    // Check if parameters are correctly set
    assert_eq!(
        protocol,
        dc.protocol(),
        "Protocol should match DataChannelConfig"
    );

    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        // Make sure this is the data channel we were looking for. (Not the one
        // created in signalPair).
        if d.label() != EXPECTED_LABEL {
            return Box::pin(async {});
        }
        // Check if parameters are correctly set
        assert_eq!(
            protocol,
            d.protocol(),
            "Protocol should match what channel creator declared"
        );

        let done_tx2 = Arc::clone(&done_tx);
        Box::pin(async move {
            let mut done = done_tx2.lock().await;
            done.take();
        })
    }));

    close_reliability_param_test(&mut offer_pc, &mut answer_pc, done_rx).await?;

    Ok(())
}

#[tokio::test]
async fn test_data_channel_parameters_negotiated_exchange() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    const EXPECTED_MESSAGE: &str = "Hello World";

    let id = 500u16;
    let options = RTCDataChannelInit {
        negotiated: Some(id),
        ..Default::default()
    };

    let (mut offer_pc, mut answer_pc, offer_datachannel, done_tx, done_rx) =
        set_up_data_channel_parameters_test(&api, Some(options.clone())).await?;

    let answer_datachannel = answer_pc
        .create_data_channel(EXPECTED_LABEL, Some(options))
        .await?;

    answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        // Ignore our default channel, exists to force ICE candidates. See signalPair for more info
        if d.label() == "initial_data_channel" {
            return Box::pin(async {});
        }
        panic!("OnDataChannel must not be fired when negotiated == true");
    }));

    offer_pc.on_data_channel(Box::new(move |_d: Arc<RTCDataChannel>| {
        panic!("OnDataChannel must not be fired when negotiated == true");
    }));

    let seen_answer_message = Arc::new(AtomicBool::new(false));
    let seen_offer_message = Arc::new(AtomicBool::new(false));

    let seen_answer_message2 = Arc::clone(&seen_answer_message);
    answer_datachannel.on_message(Box::new(move |msg: DataChannelMessage| {
        if msg.is_string && msg.data == EXPECTED_MESSAGE {
            seen_answer_message2.store(true, Ordering::SeqCst);
        }

        Box::pin(async {})
    }));

    let seen_offer_message2 = Arc::clone(&seen_offer_message);
    offer_datachannel.on_message(Box::new(move |msg: DataChannelMessage| {
        if msg.is_string && msg.data == EXPECTED_MESSAGE {
            seen_offer_message2.store(true, Ordering::SeqCst);
        }
        Box::pin(async {})
    }));

    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    tokio::spawn(async move {
        loop {
            if seen_answer_message.load(Ordering::SeqCst)
                && seen_offer_message.load(Ordering::SeqCst)
            {
                break;
            }

            if offer_datachannel.ready_state() == RTCDataChannelState::Open {
                offer_datachannel
                    .send_text(EXPECTED_MESSAGE.to_owned())
                    .await?;
            }
            if answer_datachannel.ready_state() == RTCDataChannelState::Open {
                answer_datachannel
                    .send_text(EXPECTED_MESSAGE.to_owned())
                    .await?;
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let mut done = done_tx.lock().await;
        done.take();

        Result::<()>::Ok(())
    });

    close_reliability_param_test(&mut offer_pc, &mut answer_pc, done_rx).await?;

    Ok(())
}

#[tokio::test]
async fn test_data_channel_event_handlers() -> Result<()> {
    let api = APIBuilder::new().build();

    let dc = RTCDataChannel {
        setting_engine: Arc::clone(&api.setting_engine),
        ..Default::default()
    };

    let (on_open_called_tx, mut on_open_called_rx) = mpsc::channel::<()>(1);
    let (on_message_called_tx, mut on_message_called_rx) = mpsc::channel::<()>(1);

    // Verify that the noop case works
    dc.do_open();

    let on_open_called_tx = Arc::new(Mutex::new(Some(on_open_called_tx)));
    dc.on_open(Box::new(move || {
        let on_open_called_tx2 = Arc::clone(&on_open_called_tx);
        Box::pin(async move {
            let mut done = on_open_called_tx2.lock().await;
            done.take();
        })
    }));

    let on_message_called_tx = Arc::new(Mutex::new(Some(on_message_called_tx)));
    dc.on_message(Box::new(move |_: DataChannelMessage| {
        let on_message_called_tx2 = Arc::clone(&on_message_called_tx);
        Box::pin(async move {
            let mut done = on_message_called_tx2.lock().await;
            done.take();
        })
    }));

    // Verify that the set handlers are called
    dc.do_open();
    dc.do_message(DataChannelMessage {
        is_string: false,
        data: Bytes::from_static(b"o hai"),
    })
    .await;

    // Wait for all handlers to be called
    let _ = on_open_called_rx.recv().await;
    let _ = on_message_called_rx.recv().await;

    Ok(())
}

#[tokio::test]
async fn test_data_channel_messages_are_ordered() -> Result<()> {
    let api = APIBuilder::new().build();

    let dc = RTCDataChannel {
        setting_engine: Arc::clone(&api.setting_engine),
        ..Default::default()
    };

    let m = 16u64;
    let (out_tx, mut out_rx) = mpsc::channel::<u64>(m as usize);

    let out_tx = Arc::new(out_tx);

    let out_tx1 = Arc::clone(&out_tx);
    dc.on_message(Box::new(move |msg: DataChannelMessage| {
        let out_tx2 = Arc::clone(&out_tx1);

        Box::pin(async move {
            // randomly sleep
            let r = rand::random::<u64>() % m;
            tokio::time::sleep(Duration::from_millis(r)).await;

            let mut buf = [0u8; 8];
            for i in 0..8 {
                buf[i] = msg.data[i];
            }
            let s = u64::from_be_bytes(buf);

            let _ = out_tx2.send(s).await;
        })
    }));

    tokio::spawn(async move {
        for j in 1..=m {
            let buf = j.to_be_bytes().to_vec();

            dc.do_message(DataChannelMessage {
                is_string: false,
                data: Bytes::from(buf),
            })
            .await;
            // Change the registered handler a couple of times to make sure
            // that everything continues to work, we don't lose messages, etc.
            if j % 2 == 0 {
                let out_tx1 = Arc::clone(&out_tx);
                dc.on_message(Box::new(move |msg: DataChannelMessage| {
                    let out_tx2 = Arc::clone(&out_tx1);

                    Box::pin(async move {
                        // randomly sleep
                        let r = rand::random::<u64>() % m;
                        tokio::time::sleep(Duration::from_millis(r)).await;

                        let mut buf = [0u8; 8];
                        for i in 0..8 {
                            buf[i] = msg.data[i];
                        }
                        let s = u64::from_be_bytes(buf);

                        let _ = out_tx2.send(s).await;
                    })
                }));
            }
        }
    });

    let mut values = vec![];
    for _ in 1..=m {
        if let Some(v) = out_rx.recv().await {
            values.push(v);
        } else {
            break;
        }
    }

    let mut expected = vec![0u64; m as usize];
    for i in 1..=m as usize {
        expected[i - 1] = i as u64;
    }
    assert_eq!(values, expected);

    Ok(())
}

#[tokio::test]
async fn test_data_channel_parameters_go() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    //"MaxPacketLifeTime exchange"
    {
        let ordered = true;
        let max_packet_life_time = 3u16;
        let options = RTCDataChannelInit {
            ordered: Some(ordered),
            max_packet_life_time: Some(max_packet_life_time),
            ..Default::default()
        };

        let (mut offer_pc, mut answer_pc, dc, done_tx, done_rx) =
            set_up_data_channel_parameters_test(&api, Some(options)).await?;

        // Check if parameters are correctly set
        assert!(dc.ordered(), "Ordered should be set to true");
        assert_eq!(
            Some(max_packet_life_time),
            dc.max_packet_lifetime(),
            "should match"
        );

        let done_tx = Arc::new(Mutex::new(Some(done_tx)));
        answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
            // Make sure this is the data channel we were looking for. (Not the one
            // created in signalPair).
            if d.label() != EXPECTED_LABEL {
                return Box::pin(async {});
            }

            // Check if parameters are correctly set
            assert!(d.ordered, "Ordered should be set to true");
            assert_eq!(
                Some(max_packet_life_time),
                d.max_packet_lifetime(),
                "should match"
            );

            let done_tx2 = Arc::clone(&done_tx);
            Box::pin(async move {
                let mut done = done_tx2.lock().await;
                done.take();
            })
        }));

        close_reliability_param_test(&mut offer_pc, &mut answer_pc, done_rx).await?;
    }

    //"All other property methods"
    {
        let id = 123u16;
        let dc = RTCDataChannel {
            id: AtomicU16::new(id),
            label: "mylabel".to_owned(),
            protocol: "myprotocol".to_owned(),
            negotiated: true,
            ..Default::default()
        };

        assert_eq!(dc.id.load(Ordering::SeqCst), dc.id(), "should match");
        assert_eq!(dc.label, dc.label(), "should match");
        assert_eq!(dc.protocol, dc.protocol(), "should match");
        assert_eq!(dc.negotiated, dc.negotiated(), "should match");
        assert_eq!(0, dc.buffered_amount().await, "should match");
        dc.set_buffered_amount_low_threshold(1500).await;
        assert_eq!(
            1500,
            dc.buffered_amount_low_threshold().await,
            "should match"
        );
    }

    Ok(())
}

//use log::LevelFilter;
//use std::io::Write;

#[tokio::test]
async fn test_data_channel_buffered_amount_set_before_open() -> Result<()> {
    /*env_logger::Builder::new()
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
    .filter(None, LevelFilter::Trace)
    .init();*/

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let n_cbs = Arc::new(AtomicU16::new(0));
    let buf = Bytes::from_static(&[0u8; 1000]);

    let (mut offer_pc, mut answer_pc) = new_pair(&api).await?;

    let (done_tx, done_rx) = mpsc::channel::<()>(1);

    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    let n_packets_received = Arc::new(AtomicU16::new(0));
    answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        // Make sure this is the data channel we were looking for. (Not the one
        // created in signalPair).
        if d.label() != EXPECTED_LABEL {
            return Box::pin(async {});
        }

        let done_tx2 = Arc::clone(&done_tx);
        let n_packets_received2 = Arc::clone(&n_packets_received);
        Box::pin(async move {
            d.on_message(Box::new(move |_msg: DataChannelMessage| {
                let n = n_packets_received2.fetch_add(1, Ordering::SeqCst);
                if n == 9 {
                    let done_tx3 = Arc::clone(&done_tx2);
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        let mut done = done_tx3.lock().await;
                        done.take();
                    });
                }

                Box::pin(async {})
            }));

            assert!(d.ordered(), "Ordered should be set to true");
        })
    }));

    let dc = offer_pc.create_data_channel(EXPECTED_LABEL, None).await?;

    assert!(dc.ordered(), "Ordered should be set to true");

    let dc2 = Arc::clone(&dc);
    dc.on_open(Box::new(move || {
        let dc3 = Arc::clone(&dc2);
        Box::pin(async move {
            for _ in 0..10 {
                assert!(
                    dc3.send(&buf).await.is_ok(),
                    "Failed to send string on data channel"
                );
                assert_eq!(
                    1500,
                    dc3.buffered_amount_low_threshold().await,
                    "value mismatch"
                );
            }
        })
    }));

    dc.on_message(Box::new(|_msg: DataChannelMessage| Box::pin(async {})));

    // The value is temporarily stored in the dc object
    // until the dc gets opened
    dc.set_buffered_amount_low_threshold(1500).await;
    // The callback function is temporarily stored in the dc object
    // until the dc gets opened
    let n_cbs2 = Arc::clone(&n_cbs);
    dc.on_buffered_amount_low(Box::new(move || {
        n_cbs2.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }))
    .await;

    signal_pair(&mut offer_pc, &mut answer_pc).await?;

    close_pair(&offer_pc, &answer_pc, done_rx).await;

    assert!(
        n_cbs.load(Ordering::SeqCst) > 0,
        "callback should be made at least once"
    );

    Ok(())
}

#[tokio::test]
async fn test_data_channel_buffered_amount_set_after_open() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let n_cbs = Arc::new(AtomicU16::new(0));
    let buf = Bytes::from_static(&[0u8; 1000]);

    let (mut offer_pc, mut answer_pc) = new_pair(&api).await?;

    let (done_tx, done_rx) = mpsc::channel::<()>(1);

    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    let n_packets_received = Arc::new(AtomicU16::new(0));
    answer_pc.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
        // Make sure this is the data channel we were looking for. (Not the one
        // created in signalPair).
        if d.label() != EXPECTED_LABEL {
            return Box::pin(async {});
        }

        let done_tx2 = Arc::clone(&done_tx);
        let n_packets_received2 = Arc::clone(&n_packets_received);
        Box::pin(async move {
            d.on_message(Box::new(move |_msg: DataChannelMessage| {
                let n = n_packets_received2.fetch_add(1, Ordering::SeqCst);
                if n == 9 {
                    let done_tx3 = Arc::clone(&done_tx2);
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        let mut done = done_tx3.lock().await;
                        done.take();
                    });
                }

                Box::pin(async {})
            }));

            assert!(d.ordered(), "Ordered should be set to true");
        })
    }));

    let dc = offer_pc.create_data_channel(EXPECTED_LABEL, None).await?;

    assert!(dc.ordered(), "Ordered should be set to true");

    let dc2 = Arc::clone(&dc);
    let n_cbs2 = Arc::clone(&n_cbs);
    dc.on_open(Box::new(move || {
        let dc3 = Arc::clone(&dc2);
        Box::pin(async move {
            // The value should directly be passed to sctp
            dc3.set_buffered_amount_low_threshold(1500).await;
            // The callback function should directly be passed to sctp
            dc3.on_buffered_amount_low(Box::new(move || {
                n_cbs2.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {})
            }))
            .await;

            for _ in 0..10 {
                assert!(
                    dc3.send(&buf).await.is_ok(),
                    "Failed to send string on data channel"
                );
                assert_eq!(
                    1500,
                    dc3.buffered_amount_low_threshold().await,
                    "value mismatch"
                );
            }
        })
    }));

    dc.on_message(Box::new(|_msg: DataChannelMessage| Box::pin(async {})));

    signal_pair(&mut offer_pc, &mut answer_pc).await?;

    close_pair(&offer_pc, &answer_pc, done_rx).await;

    assert!(
        n_cbs.load(Ordering::SeqCst) > 0,
        "callback should be made at least once"
    );

    Ok(())
}

#[tokio::test]
async fn test_eof_detach() -> Result<()> {
    let label: &str = "test-channel";
    let test_data: &'static str = "this is some test data";

    // Use Detach data channels mode
    let mut s = SettingEngine::default();
    s.detach_data_channels();
    let api = APIBuilder::new().with_setting_engine(s).build();

    // Set up two peer connections.
    let mut pca = api.new_peer_connection(RTCConfiguration::default()).await?;
    let mut pcb = api.new_peer_connection(RTCConfiguration::default()).await?;

    let wg = WaitGroup::new();

    let (dc_chan_tx, mut dc_chan_rx) = mpsc::channel(1);
    let dc_chan_tx = Arc::new(dc_chan_tx);
    pcb.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
        if dc.label() != label {
            return Box::pin(async {});
        }
        log::debug!("OnDataChannel was called");
        let dc_chan_tx2 = Arc::clone(&dc_chan_tx);
        let dc2 = Arc::clone(&dc);
        Box::pin(async move {
            let dc3 = Arc::clone(&dc2);
            dc2.on_open(Box::new(move || {
                let dc_chan_tx3 = Arc::clone(&dc_chan_tx2);
                let dc4 = Arc::clone(&dc3);
                Box::pin(async move {
                    let detached = match dc4.detach().await {
                        Ok(detached) => detached,
                        Err(err) => {
                            log::debug!("Detach failed: {}", err);
                            panic!();
                        }
                    };

                    let _ = dc_chan_tx3.send(detached).await;
                })
            }));
        })
    }));

    let w = wg.worker();
    tokio::spawn(async move {
        let _d = w;

        log::debug!("Waiting for OnDataChannel");
        let dc = dc_chan_rx.recv().await.unwrap();
        log::debug!("data channel opened");

        log::debug!("Waiting for ping...");
        let mut msg = vec![0u8; 256];
        let n = dc.read(&mut msg).await?;
        log::debug!("Received ping! {:?}\n", &msg[..n]);

        assert_eq!(test_data.as_bytes(), &msg[..n]);
        log::debug!("Received ping successfully!");

        dc.close().await?;

        Result::<()>::Ok(())
    });

    signal_pair(&mut pca, &mut pcb).await?;

    let attached = pca.create_data_channel(label, None).await?;

    log::debug!("Waiting for data channel to open");
    let (open_tx, mut open_rx) = mpsc::channel::<()>(1);
    let open_tx = Arc::new(open_tx);
    attached.on_open(Box::new(move || {
        let open_tx2 = Arc::clone(&open_tx);
        Box::pin(async move {
            let _ = open_tx2.send(()).await;
        })
    }));

    let _ = open_rx.recv().await;
    log::debug!("data channel opened");

    let dc = attached.detach().await?;

    let w = wg.worker();
    tokio::spawn(async move {
        let _d = w;
        log::debug!("Sending ping...");
        dc.write(&Bytes::from_static(test_data.as_bytes())).await?;
        log::debug!("Sent ping");

        dc.close().await?;

        log::debug!("Waiting for EOF");
        let mut buf = vec![0u8; 256];
        let n = dc.read(&mut buf).await?;
        assert_eq!(0, n, "should be empty");

        Result::<()>::Ok(())
    });

    wg.wait().await;

    close_pair_now(&pca, &pcb).await;

    Ok(())
}

#[tokio::test]
async fn test_eof_no_detach() -> Result<()> {
    let label: &str = "test-channel";
    let test_data: &'static [u8] = b"this is some test data";

    let api = APIBuilder::new().build();

    // Set up two peer connections.
    let mut pca = api.new_peer_connection(RTCConfiguration::default()).await?;
    let mut pcb = api.new_peer_connection(RTCConfiguration::default()).await?;

    let (dca_closed_ch_tx, mut dca_closed_ch_rx) = mpsc::channel::<()>(1);
    let (dcb_closed_ch_tx, mut dcb_closed_ch_rx) = mpsc::channel::<()>(1);

    let dcb_closed_ch_tx = Arc::new(dcb_closed_ch_tx);
    pcb.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
        if dc.label() != label {
            return Box::pin(async {});
        }

        log::debug!("pcb: new datachannel: {}", dc.label());

        let dcb_closed_ch_tx2 = Arc::clone(&dcb_closed_ch_tx);
        Box::pin(async move {
            // Register channel opening handling
            dc.on_open(Box::new(move || {
                log::debug!("pcb: datachannel opened");
                Box::pin(async {})
            }));

            dc.on_close(Box::new(move || {
                // (2)
                log::debug!("pcb: data channel closed");
                let dcb_closed_ch_tx3 = Arc::clone(&dcb_closed_ch_tx2);
                Box::pin(async move {
                    let _ = dcb_closed_ch_tx3.send(()).await;
                })
            }));

            // Register the OnMessage to handle incoming messages
            log::debug!("pcb: registering onMessage callback");
            dc.on_message(Box::new(|dc_msg: DataChannelMessage| {
                let test_data: &'static [u8] = b"this is some test data";
                log::debug!("pcb: received ping: {:?}", dc_msg.data);
                assert_eq!(&dc_msg.data[..], test_data, "data mismatch");
                Box::pin(async {})
            }));
        })
    }));

    let dca = pca.create_data_channel(label, None).await?;
    let dca2 = Arc::clone(&dca);
    dca.on_open(Box::new(move || {
        log::debug!("pca: data channel opened");
        log::debug!("pca: sending {:?}", test_data);
        let dca3 = Arc::clone(&dca2);
        Box::pin(async move {
            let _ = dca3.send(&Bytes::from_static(test_data)).await;
            log::debug!("pca: sent ping");
            assert!(dca3.close().await.is_ok(), "should succeed"); // <-- dca closes
        })
    }));

    let dca_closed_ch_tx = Arc::new(dca_closed_ch_tx);
    dca.on_close(Box::new(move || {
        // (1)
        log::debug!("pca: data channel closed");
        let dca_closed_ch_tx2 = Arc::clone(&dca_closed_ch_tx);
        Box::pin(async move {
            let _ = dca_closed_ch_tx2.send(()).await;
        })
    }));

    // Register the OnMessage to handle incoming messages
    log::debug!("pca: registering onMessage callback");
    dca.on_message(Box::new(move |dc_msg: DataChannelMessage| {
        log::debug!("pca: received pong: {:?}", &dc_msg.data[..]);
        assert_eq!(&dc_msg.data[..], test_data, "data mismatch");
        Box::pin(async {})
    }));

    signal_pair(&mut pca, &mut pcb).await?;

    // When dca closes the channel,
    // (1) dca.Onclose() will fire immediately, then
    // (2) dcb.OnClose will also fire
    let _ = dca_closed_ch_rx.recv().await; // (1)
    let _ = dcb_closed_ch_rx.recv().await; // (2)

    close_pair_now(&pca, &pcb).await;

    Ok(())
}

// Assert that a Session Description that doesn't follow
// draft-ietf-mmusic-sctp-sdp is still accepted
#[tokio::test]
async fn test_data_channel_non_standard_session_description() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (offer_pc, answer_pc) = new_pair(&api).await?;

    let _ = offer_pc.create_data_channel("foo", None).await?;

    let (on_data_channel_called_tx, mut on_data_channel_called_rx) = mpsc::channel::<()>(1);
    let on_data_channel_called_tx = Arc::new(on_data_channel_called_tx);
    answer_pc.on_data_channel(Box::new(move |_: Arc<RTCDataChannel>| {
        let on_data_channel_called_tx2 = Arc::clone(&on_data_channel_called_tx);
        Box::pin(async move {
            let _ = on_data_channel_called_tx2.send(()).await;
        })
    }));

    let offer = offer_pc.create_offer(None).await?;

    let mut offer_gathering_complete = offer_pc.gathering_complete_promise().await;
    offer_pc.set_local_description(offer).await?;
    let _ = offer_gathering_complete.recv().await;

    let mut offer = offer_pc.local_description().await.unwrap();

    // Replace with old values
    const OLD_APPLICATION: &str = "m=application 63743 DTLS/SCTP 5000\r";
    const OLD_ATTRIBUTE: &str = "a=sctpmap:5000 webrtc-datachannel 256\r";

    let re = Regex::new(r"m=application (.*?)\r").unwrap();
    offer.sdp = re
        .replace_all(offer.sdp.as_str(), OLD_APPLICATION)
        .to_string();
    let re = Regex::new(r"a=sctp-port(.*?)\r").unwrap();
    offer.sdp = re
        .replace_all(offer.sdp.as_str(), OLD_ATTRIBUTE)
        .to_string();

    // Assert that replace worked
    assert!(offer.sdp.contains(OLD_APPLICATION));
    assert!(offer.sdp.contains(OLD_ATTRIBUTE));

    answer_pc.set_remote_description(offer).await?;

    let answer = answer_pc.create_answer(None).await?;

    let mut answer_gathering_complete = answer_pc.gathering_complete_promise().await;
    answer_pc.set_local_description(answer).await?;
    let _ = answer_gathering_complete.recv().await;

    let answer = answer_pc.local_description().await.unwrap();
    offer_pc.set_remote_description(answer).await?;

    let _ = on_data_channel_called_rx.recv().await;

    close_pair_now(&offer_pc, &answer_pc).await;

    Ok(())
}

struct TestOrtcStack {
    //api      *API
    gatherer: Arc<RTCIceGatherer>,
    ice: Arc<RTCIceTransport>,
    dtls: Arc<RTCDtlsTransport>,
    sctp: Arc<RTCSctpTransport>,
}

struct TestOrtcSignal {
    ice_candidates: Vec<RTCIceCandidate>, //`json:"iceCandidates"`
    ice_parameters: RTCIceParameters,     //`json:"iceParameters"`
    dtls_parameters: DTLSParameters,      //`json:"dtlsParameters"`
    sctp_capabilities: SCTPTransportCapabilities, //`json:"sctpCapabilities"`
}

impl TestOrtcStack {
    async fn new(api: &API) -> Result<Self> {
        // Create the ICE gatherer
        let gatherer = Arc::new(api.new_ice_gatherer(RTCIceGatherOptions::default())?);

        // Construct the ICE transport
        let ice = Arc::new(api.new_ice_transport(Arc::clone(&gatherer)));

        // Construct the DTLS transport
        let dtls = Arc::new(api.new_dtls_transport(Arc::clone(&ice), vec![])?);

        // Construct the SCTP transport
        let sctp = Arc::new(api.new_sctp_transport(Arc::clone(&dtls))?);

        Ok(TestOrtcStack {
            gatherer,
            ice,
            dtls,
            sctp,
        })
    }

    async fn set_signal(&self, sig: &TestOrtcSignal, is_offer: bool) -> Result<()> {
        let ice_role = if is_offer {
            RTCIceRole::Controlling
        } else {
            RTCIceRole::Controlled
        };

        self.ice.set_remote_candidates(&sig.ice_candidates).await?;

        // Start the ICE transport
        self.ice.start(&sig.ice_parameters, Some(ice_role)).await?;

        // Start the DTLS transport
        self.dtls.start(sig.dtls_parameters.clone()).await?;

        // Start the SCTP transport
        self.sctp.start(sig.sctp_capabilities).await?;

        Ok(())
    }

    async fn get_signal(&self) -> Result<TestOrtcSignal> {
        let (gather_finished_tx, mut gather_finished_rx) = mpsc::channel::<()>(1);
        let gather_finished_tx = Arc::new(gather_finished_tx);
        self.gatherer
            .on_local_candidate(Box::new(move |i: Option<RTCIceCandidate>| {
                let gather_finished_tx2 = Arc::clone(&gather_finished_tx);
                Box::pin(async move {
                    if i.is_none() {
                        let _ = gather_finished_tx2.send(()).await;
                    }
                })
            }));

        self.gatherer.gather().await?;

        let _ = gather_finished_rx.recv().await;

        let ice_candidates = self.gatherer.get_local_candidates().await?;

        let ice_parameters = self.gatherer.get_local_parameters().await?;

        let dtls_parameters = self.dtls.get_local_parameters()?;

        let sctp_capabilities = self.sctp.get_capabilities();

        Ok(TestOrtcSignal {
            ice_candidates,
            ice_parameters,
            dtls_parameters,
            sctp_capabilities,
        })
    }

    async fn close(&self) -> Result<()> {
        let mut close_errs = vec![];

        if let Err(err) = self.sctp.stop().await {
            close_errs.push(err);
        }

        if let Err(err) = self.ice.stop().await {
            close_errs.push(err);
        }

        flatten_errs(close_errs)
    }
}

async fn new_ortc_pair(api: &API) -> Result<(Arc<TestOrtcStack>, Arc<TestOrtcStack>)> {
    let sa = Arc::new(TestOrtcStack::new(api).await?);
    let sb = Arc::new(TestOrtcStack::new(api).await?);
    Ok((sa, sb))
}

async fn signal_ortc_pair(stack_a: Arc<TestOrtcStack>, stack_b: Arc<TestOrtcStack>) -> Result<()> {
    let sig_a = stack_a.get_signal().await?;
    let sig_b = stack_b.get_signal().await?;

    let (a_tx, mut a_rx) = mpsc::channel(1);
    let (b_tx, mut b_rx) = mpsc::channel(1);

    tokio::spawn(async move {
        let _ = a_tx.send(stack_b.set_signal(&sig_a, false).await).await;
    });

    tokio::spawn(async move {
        let _ = b_tx.send(stack_a.set_signal(&sig_b, true).await).await;
    });

    let err_a = a_rx.recv().await.unwrap();
    let err_b = b_rx.recv().await.unwrap();

    let mut close_errs = vec![];
    if let Err(err) = err_a {
        close_errs.push(err);
    }
    if let Err(err) = err_b {
        close_errs.push(err);
    }

    flatten_errs(close_errs)
}

#[tokio::test]
async fn test_data_channel_ortc_e2e() -> Result<()> {
    let api = APIBuilder::new().build();

    let (stack_a, stack_b) = new_ortc_pair(&api).await?;

    let (await_setup_tx, mut await_setup_rx) = mpsc::channel::<()>(1);
    let (await_string_tx, mut await_string_rx) = mpsc::channel::<()>(1);
    let (await_binary_tx, mut await_binary_rx) = mpsc::channel::<()>(1);

    let await_setup_tx = Arc::new(await_setup_tx);
    let await_string_tx = Arc::new(await_string_tx);
    let await_binary_tx = Arc::new(await_binary_tx);
    stack_b
        .sctp
        .on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
            let await_setup_tx2 = Arc::clone(&await_setup_tx);
            let await_string_tx2 = Arc::clone(&await_string_tx);
            let await_binary_tx2 = Arc::clone(&await_binary_tx);
            Box::pin(async move {
                let _ = await_setup_tx2.send(()).await;

                d.on_message(Box::new(move |msg: DataChannelMessage| {
                    let await_string_tx3 = Arc::clone(&await_string_tx2);
                    let await_binary_tx3 = Arc::clone(&await_binary_tx2);
                    Box::pin(async move {
                        if msg.is_string {
                            let _ = await_string_tx3.send(()).await;
                        } else {
                            let _ = await_binary_tx3.send(()).await;
                        }
                    })
                }));
            })
        }));

    signal_ortc_pair(Arc::clone(&stack_a), Arc::clone(&stack_b)).await?;

    let dc_params = DataChannelParameters {
        label: "Foo".to_owned(),
        negotiated: None,
        ..Default::default()
    };

    let channel_a = api
        .new_data_channel(Arc::clone(&stack_a.sctp), dc_params)
        .await?;

    let _ = await_setup_rx.recv().await;

    channel_a.send_text("ABC".to_owned()).await?;
    channel_a.send(&Bytes::from_static(b"ABC")).await?;

    let _ = await_string_rx.recv().await;
    let _ = await_binary_rx.recv().await;

    stack_a.close().await?;
    stack_b.close().await?;

    // attempt to send when channel is closed
    let result = channel_a.send(&Bytes::from_static(b"ABC")).await;
    if let Err(err) = result {
        assert_eq!(
            Error::ErrClosedPipe,
            err,
            "expected ErrClosedPipe, but got {err}"
        );
    } else {
        panic!();
    }

    let result = channel_a.send_text("test".to_owned()).await;
    if let Err(err) = result {
        assert_eq!(
            Error::ErrClosedPipe,
            err,
            "expected ErrClosedPipe, but got {err}"
        );
    } else {
        panic!();
    }

    let result = channel_a.ensure_open();
    if let Err(err) = result {
        assert_eq!(
            Error::ErrClosedPipe,
            err,
            "expected ErrClosedPipe, but got {err}"
        );
    } else {
        panic!();
    }

    Ok(())
}
