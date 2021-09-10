use super::*;
use crate::api::media_engine::MediaEngine;
use crate::api::{APIBuilder, API};
use crate::data::data_channel::data_channel_config::DataChannelConfig;
use crate::peer::peer_connection::peer_connection_test::*;
use crate::peer::peer_connection::PeerConnection;

//use log::LevelFilter;
//use std::io::Write;
use crate::peer::ice::ice_connection_state::ICEConnectionState;
use tokio::sync::mpsc;
use tokio::time::Duration;

// EXPECTED_LABEL represents the label of the data channel we are trying to test.
// Some other channels may have been created during initialization (in the Wasm
// bindings this is a requirement).
const EXPECTED_LABEL: &str = "data";

async fn set_up_data_channel_parameters_test(
    api: &API,
    options: Option<DataChannelConfig>,
) -> Result<(
    PeerConnection,
    PeerConnection,
    Arc<DataChannel>,
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
    pc1: &mut PeerConnection,
    pc2: &mut PeerConnection,
    done_rx: mpsc::Receiver<()>,
) -> Result<()> {
    signal_pair(pc1, pc2).await?;

    close_pair(pc1, pc2, done_rx).await;

    Ok(())
}

/*
TODO: func BenchmarkDataChannelSend2(b *testing.B)  { benchmarkDataChannelSend(b, 2) }
func BenchmarkDataChannelSend4(b *testing.B)  { benchmarkDataChannelSend(b, 4) }
func BenchmarkDataChannelSend8(b *testing.B)  { benchmarkDataChannelSend(b, 8) }
func BenchmarkDataChannelSend16(b *testing.B) { benchmarkDataChannelSend(b, 16) }
func BenchmarkDataChannelSend32(b *testing.B) { benchmarkDataChannelSend(b, 32) }

// See https://github.com/pion/webrtc/issues/1516
func benchmarkDataChannelSend(b *testing.B, numChannels int) {
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
        answer_pc
            .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
                if d.label() == EXPECTED_LABEL {
                    let open_calls_tx2 = Arc::clone(&open_calls_tx);
                    let done_tx2 = Arc::clone(&done_tx);
                    Box::pin(async move {
                        d.on_open(Box::new(move || {
                            Box::pin(async move {
                                let _ = open_calls_tx2.send(()).await;
                            })
                        }))
                        .await;
                        d.on_message(Box::new(move |_: DataChannelMessage| {
                            let done_tx3 = Arc::clone(&done_tx2);
                            tokio::spawn(async move {
                                // Wait a little bit to ensure all messages are processed.
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                let _ = done_tx3.send(()).await;
                            });
                            Box::pin(async {})
                        }))
                        .await;
                    })
                } else {
                    Box::pin(async {})
                }
            }))
            .await;

        let dc = offer_pc.create_data_channel(EXPECTED_LABEL, None).await?;

        let dc2 = Arc::clone(&dc);
        dc.on_open(Box::new(move || {
            Box::pin(async move {
                let result = dc2.send_text("Ping".to_owned()).await;
                assert!(result.is_ok(), "Failed to send string on data channel");
            })
        }))
        .await;

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

    answer_pc
        .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
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
                }))
                .await;
                assert!(d.ordered(), "Ordered should be set to true");
            })
        }))
        .await;

    let dc = offer_pc.create_data_channel(EXPECTED_LABEL, None).await?;

    assert!(dc.ordered(), "Ordered should be set to true");

    let dc2 = Arc::clone(&dc);
    dc.on_open(Box::new(move || {
        let dc3 = Arc::clone(&dc2);
        Box::pin(async move {
            let result = dc3.send_text("Ping".to_owned()).await;
            assert!(result.is_ok(), "Failed to send string on data channel");
        })
    }))
    .await;

    let (done_tx, done_rx) = mpsc::channel::<()>(1);
    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    dc.on_message(Box::new(move |_: DataChannelMessage| {
        let done_tx2 = Arc::clone(&done_tx);
        Box::pin(async move {
            let mut done = done_tx2.lock().await;
            done.take();
        })
    }))
    .await;

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

    answer_pc
        .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
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
                }))
                .await;
                assert!(d.ordered(), "Ordered should be set to true");
            })
        }))
        .await;

    let dc = match offer_pc.create_data_channel(EXPECTED_LABEL, None).await {
        Ok(dc) => dc,
        Err(_) => {
            assert!(false, "Failed to create a PC pair for testing");
            return Ok(());
        }
    };

    let (done_tx, done_rx) = mpsc::channel::<()>(1);
    let done_tx = Arc::new(Mutex::new(Some(done_tx)));

    //once := &sync.Once{}
    offer_pc
        .on_ice_connection_state_change(Box::new(move |state: ICEConnectionState| {
            let done_tx1 = Arc::clone(&done_tx);
            let dc1 = Arc::clone(&dc);
            Box::pin(async move {
                if state == ICEConnectionState::Connected || state == ICEConnectionState::Completed
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
                        }))
                        .await;

                        if let Err(_) = dc1.send_text("Ping".to_owned()).await {
                            // wasm binding doesn't fire OnOpen (we probably already missed it)
                            let dc2 = Arc::clone(&dc1);
                            dc1.on_open(Box::new(move || {
                                let dc3 = Arc::clone(&dc2);
                                Box::pin(async move {
                                    let result = dc3.send_text("Ping".to_owned()).await;
                                    assert!(
                                        result.is_ok(),
                                        "Failed to send string on data channel"
                                    );
                                })
                            }))
                            .await;
                        }
                    }
                }
            })
        }))
        .await;

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
        let (mut offer_pc, mut answer_pc) = new_pair(&api).await?;

        let dc = offer_pc.create_data_channel(EXPECTED_LABEL, None).await?;

        close_pair_now(&mut offer_pc, &mut answer_pc).await;
        dc.close().await?;
    }

    // "Close before connected"
    {
        let (mut offer_pc, mut answer_pc) = new_pair(&api).await?;

        let dc = offer_pc.create_data_channel(EXPECTED_LABEL, None).await?;

        dc.close().await?;
        close_pair_now(&mut offer_pc, &mut answer_pc).await;
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
    let options = DataChannelConfig {
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
        max_packet_life_time,
        "should match"
    );

    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    answer_pc
        .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
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
                max_packet_life_time,
                "should match"
            );
            let done_tx2 = Arc::clone(&done_tx);
            Box::pin(async move {
                let mut done = done_tx2.lock().await;
                done.take();
            })
        }))
        .await;

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
    let options = DataChannelConfig {
        ordered: Some(ordered),
        max_retransmits: Some(max_retransmits),
        ..Default::default()
    };

    let (mut offer_pc, mut answer_pc, dc, done_tx, done_rx) =
        set_up_data_channel_parameters_test(&api, Some(options)).await?;

    // Check if parameters are correctly set
    assert!(!dc.ordered(), "Ordered should be set to false");
    assert_eq!(dc.max_retransmits(), max_retransmits, "should match");

    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    answer_pc
        .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
            // Make sure this is the data channel we were looking for. (Not the one
            // created in signalPair).
            if d.label() != EXPECTED_LABEL {
                return Box::pin(async {});
            }

            // Check if parameters are correctly set
            assert!(!d.ordered(), "Ordered should be set to false");
            assert_eq!(max_retransmits, d.max_retransmits(), "should match");
            let done_tx2 = Arc::clone(&done_tx);
            Box::pin(async move {
                let mut done = done_tx2.lock().await;
                done.take();
            })
        }))
        .await;

    close_reliability_param_test(&mut offer_pc, &mut answer_pc, done_rx).await?;

    Ok(())
}

#[tokio::test]
async fn test_data_channel_parameters_protocol_exchange() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let protocol = "json".to_owned();
    let options = DataChannelConfig {
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
    answer_pc
        .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
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
        }))
        .await;

    close_reliability_param_test(&mut offer_pc, &mut answer_pc, done_rx).await?;

    Ok(())
}

#[tokio::test]
async fn test_data_channel_parameters_negotiated_exchange() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    const EXPECTED_MESSAGE: &str = "Hello World";

    let negotiated = true;
    let id = 500u16;
    let options = DataChannelConfig {
        negotiated: Some(negotiated),
        id: Some(id),
        ..Default::default()
    };

    let (mut offer_pc, mut answer_pc, offer_datachannel, done_tx, done_rx) =
        set_up_data_channel_parameters_test(&api, Some(options.clone())).await?;

    let answer_datachannel = answer_pc
        .create_data_channel(EXPECTED_LABEL, Some(options))
        .await?;

    answer_pc
        .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
            // Ignore our default channel, exists to force ICE candidates. See signalPair for more info
            if d.label() == "initial_data_channel" {
                return Box::pin(async {});
            }
            assert!(
                false,
                "OnDataChannel must not be fired when negotiated == true"
            );

            Box::pin(async {})
        }))
        .await;

    offer_pc
        .on_data_channel(Box::new(move |_d: Arc<DataChannel>| {
            assert!(
                false,
                "OnDataChannel must not be fired when negotiated == true"
            );

            Box::pin(async {})
        }))
        .await;

    let seen_answer_message = Arc::new(AtomicBool::new(false));
    let seen_offer_message = Arc::new(AtomicBool::new(false));

    let seen_answer_message2 = Arc::clone(&seen_answer_message);
    answer_datachannel
        .on_message(Box::new(move |msg: DataChannelMessage| {
            if msg.is_string && msg.data == EXPECTED_MESSAGE {
                seen_answer_message2.store(true, Ordering::SeqCst);
            }

            Box::pin(async {})
        }))
        .await;

    let seen_offer_message2 = Arc::clone(&seen_offer_message);
    offer_datachannel
        .on_message(Box::new(move |msg: DataChannelMessage| {
            if msg.is_string && msg.data == EXPECTED_MESSAGE {
                seen_offer_message2.store(true, Ordering::SeqCst);
            }
            Box::pin(async {})
        }))
        .await;

    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    tokio::spawn(async move {
        loop {
            if seen_answer_message.load(Ordering::SeqCst)
                && seen_offer_message.load(Ordering::SeqCst)
            {
                break;
            }

            if offer_datachannel.ready_state() == DataChannelState::Open {
                offer_datachannel
                    .send_text(EXPECTED_MESSAGE.to_owned())
                    .await?;
            }
            if answer_datachannel.ready_state() == DataChannelState::Open {
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

//TODO: add datachannel_go_test
//TODO: add datachannel_ortc_test
