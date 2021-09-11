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

#[tokio::test]
async fn test_data_channel_event_handlers() -> Result<()> {
    let api = APIBuilder::new().build();

    let dc = DataChannel {
        setting_engine: Arc::clone(&api.setting_engine),
        ..Default::default()
    };

    let (on_open_called_tx, mut on_open_called_rx) = mpsc::channel::<()>(1);
    let (on_message_called_tx, mut on_message_called_rx) = mpsc::channel::<()>(1);

    // Verify that the noop case works
    dc.do_open().await;

    let on_open_called_tx = Arc::new(Mutex::new(Some(on_open_called_tx)));
    dc.on_open(Box::new(move || {
        let on_open_called_tx2 = Arc::clone(&on_open_called_tx);
        Box::pin(async move {
            let mut done = on_open_called_tx2.lock().await;
            done.take();
        })
    }))
    .await;

    let on_message_called_tx = Arc::new(Mutex::new(Some(on_message_called_tx)));
    dc.on_message(Box::new(move |_: DataChannelMessage| {
        let on_message_called_tx2 = Arc::clone(&on_message_called_tx);
        Box::pin(async move {
            let mut done = on_message_called_tx2.lock().await;
            done.take();
        })
    }))
    .await;

    // Verify that the set handlers are called
    dc.do_open().await;
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

    let dc = DataChannel {
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
    }))
    .await;

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
                }))
                .await;
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
    assert_eq!(expected, values);

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
        let options = DataChannelConfig {
            ordered: Some(ordered),
            max_packet_life_time: Some(max_packet_life_time),
            ..Default::default()
        };

        let (mut offer_pc, mut answer_pc, dc, done_tx, done_rx) =
            set_up_data_channel_parameters_test(&api, Some(options)).await?;

        // Check if parameters are correctly set
        assert!(dc.ordered(), "Ordered should be set to true");
        assert_eq!(
            max_packet_life_time,
            dc.max_packet_lifetime(),
            "should match"
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
                assert!(d.ordered, "Ordered should be set to true");
                assert_eq!(
                    max_packet_life_time,
                    d.max_packet_lifetime(),
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
    }

    //"All other property methods"
    {
        let id = 123u16;
        let dc = DataChannel {
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

#[tokio::test]
async fn test_data_channel_buffered_amount() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    //"set before datachannel becomes open"
    {
        let n_cbs = Arc::new(AtomicU16::new(0));
        let buf = Bytes::from_static(&[0u8; 1000]);

        let (mut offer_pc, mut answer_pc) = new_pair(&api).await?;

        let (done_tx, done_rx) = mpsc::channel::<()>(1);

        let done_tx = Arc::new(Mutex::new(Some(done_tx)));
        let n_packets_received = Arc::new(AtomicU16::new(0));
        answer_pc
            .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
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
                for _ in 0..10 {
                    if let Err(_) = dc3.send(&buf).await {
                        assert!(false, "Failed to send string on data channel");
                    }
                    assert_eq!(
                        1500,
                        dc3.buffered_amount_low_threshold().await,
                        "value mismatch"
                    );
                }
            })
        }))
        .await;

        dc.on_message(Box::new(|_msg: DataChannelMessage| Box::pin(async {})))
            .await;

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

        /*TODO: FIXME: assert!(
            n_cbs.load(Ordering::SeqCst) > 0,
            "callback should be made at least once"
        );*/
    }

    //"set after datachannel becomes open"
    {
        let n_cbs = Arc::new(AtomicU16::new(0));
        let buf = Bytes::from_static(&[0u8; 1000]);

        let (mut offer_pc, mut answer_pc) = new_pair(&api).await?;

        let (done_tx, done_rx) = mpsc::channel::<()>(1);

        let done_tx = Arc::new(Mutex::new(Some(done_tx)));
        let n_packets_received = Arc::new(AtomicU16::new(0));
        answer_pc
            .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
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
                    }))
                    .await;

                    assert!(d.ordered(), "Ordered should be set to true");
                })
            }))
            .await;

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
                    if let Err(_) = dc3.send(&buf).await {
                        assert!(false, "Failed to send string on data channel");
                    }
                    assert_eq!(
                        1500,
                        dc3.buffered_amount_low_threshold().await,
                        "value mismatch"
                    );
                }
            })
        }))
        .await;

        dc.on_message(Box::new(|_msg: DataChannelMessage| Box::pin(async {})))
            .await;

        signal_pair(&mut offer_pc, &mut answer_pc).await?;

        close_pair(&offer_pc, &answer_pc, done_rx).await;

        assert!(
            n_cbs.load(Ordering::SeqCst) > 0,
            "callback should be made at least once"
        );
    }

    Ok(())
}

/*
#[tokio::test] async fn TestEOF()->Result<()> {
    report := test.CheckRoutines(t)
    defer report()

    log := logging.NewDefaultLoggerFactory().NewLogger("test")
    label := "test-channel"
    testData := []byte("this is some test data")

    t.Run("Detach", func()->Result<()> {
        // Use Detach data channels mode
        s := SettingEngine{}
        s.DetachDataChannels()
        api := NewAPI(WithSettingEngine(s))

        // Set up two peer connections.
        config := Configuration{}
        pca, err := api.NewPeerConnection(config)
        if err != nil {
            t.Fatal(err)
        }
        pcb, err := api.NewPeerConnection(config)
        if err != nil {
            t.Fatal(err)
        }

        defer closePairNow(t, pca, pcb)

        var wg sync.WaitGroup

        dcChan := make(chan datachannel.ReadWriteCloser)
        pcb.OnDataChannel(func(dc *DataChannel) {
            if dc.Label() != label {
                return
            }
            log.Debug("OnDataChannel was called")
            dc.OnOpen(func() {
                detached, err2 := dc.Detach()
                if err2 != nil {
                    log.Debugf("Detach failed: %s\n", err2.Error())
                    t.Error(err2)
                }

                dcChan <- detached
            })
        })

        wg.Add(1)
        go func() {
            defer wg.Done()

            var msg []byte

            log.Debug("Waiting for OnDataChannel")
            dc := <-dcChan
            log.Debug("data channel opened")
            defer func() { assert.NoError(t, dc.Close(), "should succeed") }()

            log.Debug("Waiting for ping...")
            msg, err2 := ioutil.ReadAll(dc)
            log.Debugf("Received ping! \"%s\"\n", string(msg))
            if err2 != nil {
                t.Error(err2)
            }

            if !bytes.Equal(msg, testData) {
                t.Errorf("expected %q, got %q", string(msg), string(testData))
            } else {
                log.Debug("Received ping successfully!")
            }
        }()

        if err = signalPair(pca, pcb); err != nil {
            t.Fatal(err)
        }

        attached, err := pca.CreateDataChannel(label, nil)
        if err != nil {
            t.Fatal(err)
        }
        log.Debug("Waiting for data channel to open")
        open := make(chan struct{})
        attached.OnOpen(func() {
            open <- struct{}{}
        })
        <-open
        log.Debug("data channel opened")

        var dc io.ReadWriteCloser
        dc, err = attached.Detach()
        if err != nil {
            t.Fatal(err)
        }

        wg.Add(1)
        go func() {
            defer wg.Done()
            log.Debug("Sending ping...")
            if _, err2 := dc.Write(testData); err2 != nil {
                t.Error(err2)
            }
            log.Debug("Sent ping")

            assert.NoError(t, dc.Close(), "should succeed")

            log.Debug("Wating for EOF")
            ret, err2 := ioutil.ReadAll(dc)
            assert.Nil(t, err2, "should succeed")
            assert.Equal(t, 0, len(ret), "should be empty")
        }()

        wg.Wait()
    })

    t.Run("No detach", func()->Result<()> {
        lim := test.TimeOut(time.Second * 5)
        defer lim.Stop()

        // Set up two peer connections.
        config := Configuration{}
        pca, err := NewPeerConnection(config)
        if err != nil {
            t.Fatal(err)
        }
        pcb, err := NewPeerConnection(config)
        if err != nil {
            t.Fatal(err)
        }

        defer closePairNow(t, pca, pcb)

        var dca, dcb *DataChannel
        dcaClosedCh := make(chan struct{})
        dcbClosedCh := make(chan struct{})

        pcb.OnDataChannel(func(dc *DataChannel) {
            if dc.Label() != label {
                return
            }

            log.Debugf("pcb: new datachannel: %s\n", dc.Label())

            dcb = dc
            // Register channel opening handling
            dcb.OnOpen(func() {
                log.Debug("pcb: datachannel opened")
            })

            dcb.OnClose(func() {
                // (2)
                log.Debug("pcb: data channel closed")
                close(dcbClosedCh)
            })

            // Register the OnMessage to handle incoming messages
            log.Debug("pcb: registering onMessage callback")
            dcb.OnMessage(func(dcMsg DataChannelMessage) {
                log.Debugf("pcb: received ping: %s\n", string(dcMsg.Data))
                if !reflect.DeepEqual(dcMsg.Data, testData) {
                    t.Error("data mismatch")
                }
            })
        })

        dca, err = pca.CreateDataChannel(label, nil)
        if err != nil {
            t.Fatal(err)
        }

        dca.OnOpen(func() {
            log.Debug("pca: data channel opened")
            log.Debugf("pca: sending \"%s\"", string(testData))
            if err := dca.Send(testData); err != nil {
                t.Fatal(err)
            }
            log.Debug("pca: sent ping")
            assert.NoError(t, dca.Close(), "should succeed") // <-- dca closes
        })

        dca.OnClose(func() {
            // (1)
            log.Debug("pca: data channel closed")
            close(dcaClosedCh)
        })

        // Register the OnMessage to handle incoming messages
        log.Debug("pca: registering onMessage callback")
        dca.OnMessage(func(dcMsg DataChannelMessage) {
            log.Debugf("pca: received pong: %s\n", string(dcMsg.Data))
            if !reflect.DeepEqual(dcMsg.Data, testData) {
                t.Error("data mismatch")
            }
        })

        if err := signalPair(pca, pcb); err != nil {
            t.Fatal(err)
        }

        // When dca closes the channel,
        // (1) dca.Onclose() will fire immediately, then
        // (2) dcb.OnClose will also fire
        <-dcaClosedCh // (1)
        <-dcbClosedCh // (2)
    })
}

// Assert that a Session Description that doesn't follow
// draft-ietf-mmusic-sctp-sdp is still accepted
#[tokio::test] async fn TestDataChannel_NonStandardSessionDescription()->Result<()> {
    to := test.TimeOut(time.Second * 20)
    defer to.Stop()

    report := test.CheckRoutines(t)
    defer report()

    offerPC, answerPC, err := newPair()
    assert.NoError(t, err)

    _, err = offerPC.CreateDataChannel("foo", nil)
    assert.NoError(t, err)

    onDataChannelCalled := make(chan struct{})
    answerPC.OnDataChannel(func(_ *DataChannel) {
        close(onDataChannelCalled)
    })

    offer, err := offerPC.CreateOffer(nil)
    assert.NoError(t, err)

    offerGatheringComplete := GatheringCompletePromise(offerPC)
    assert.NoError(t, offerPC.SetLocalDescription(offer))
    <-offerGatheringComplete

    offer = *offerPC.LocalDescription()

    // Replace with old values
    const (
        oldApplication = "m=application 63743 DTLS/SCTP 5000\r"
        oldAttribute   = "a=sctpmap:5000 webrtc-datachannel 256\r"
    )

    offer.SDP = regexp.MustCompile(`m=application (.*?)\r`).ReplaceAllString(offer.SDP, oldApplication)
    offer.SDP = regexp.MustCompile(`a=sctp-port(.*?)\r`).ReplaceAllString(offer.SDP, oldAttribute)

    // Assert that replace worked
    assert.True(t, strings.Contains(offer.SDP, oldApplication))
    assert.True(t, strings.Contains(offer.SDP, oldAttribute))

    assert.NoError(t, answerPC.SetRemoteDescription(offer))

    answer, err := answerPC.CreateAnswer(nil)
    assert.NoError(t, err)

    answerGatheringComplete := GatheringCompletePromise(answerPC)
    assert.NoError(t, answerPC.SetLocalDescription(answer))
    <-answerGatheringComplete
    assert.NoError(t, offerPC.SetRemoteDescription(*answerPC.LocalDescription()))

    <-onDataChannelCalled
    closePairNow(t, offerPC, answerPC)
}
*/

//TODO: add datachannel_ortc_test
/*
#[tokio::test] async fn TestDataChannel_ORTCE2E()->Result<()> {
    // Limit runtime in case of deadlocks
    lim := test.TimeOut(time.Second * 20)
    defer lim.Stop()

    report := test.CheckRoutines(t)
    defer report()

    stackA, stackB, err := newORTCPair()
    if err != nil {
        t.Fatal(err)
    }

    awaitSetup := make(chan struct{})
    awaitString := make(chan struct{})
    awaitBinary := make(chan struct{})
    stackB.sctp.OnDataChannel(func(d *DataChannel) {
        close(awaitSetup)

        d.OnMessage(func(msg DataChannelMessage) {
            if msg.IsString {
                close(awaitString)
            } else {
                close(awaitBinary)
            }
        })
    })

    err = signalORTCPair(stackA, stackB)
    if err != nil {
        t.Fatal(err)
    }

    var id uint16 = 1
    dcParams := &DataChannelParameters{
        Label: "Foo",
        ID:    &id,
    }
    channelA, err := stackA.api.NewDataChannel(stackA.sctp, dcParams)
    if err != nil {
        t.Fatal(err)
    }

    <-awaitSetup

    err = channelA.SendText("ABC")
    if err != nil {
        t.Fatal(err)
    }
    err = channelA.Send([]byte("ABC"))
    if err != nil {
        t.Fatal(err)
    }
    <-awaitString
    <-awaitBinary

    err = stackA.close()
    if err != nil {
        t.Fatal(err)
    }

    err = stackB.close()
    if err != nil {
        t.Fatal(err)
    }

    // attempt to send when channel is closed
    err = channelA.Send([]byte("ABC"))
    assert.Error(t, err)
    assert.Equal(t, io.ErrClosedPipe, err)

    err = channelA.SendText("test")
    assert.Error(t, err)
    assert.Equal(t, io.ErrClosedPipe, err)

    err = channelA.ensureOpen()
    assert.Error(t, err)
    assert.Equal(t, io.ErrClosedPipe, err)
}

type testORTCStack struct {
    api      *API
    gatherer *ICEGatherer
    ice      *ICETransport
    dtls     *DTLSTransport
    sctp     *SCTPTransport
}

#[tokio::test] async fn(s *testORTCStack) setSignal(sig *testORTCSignal, isOffer bool) error {
    iceRole := ICERoleControlled
    if isOffer {
        iceRole = ICERoleControlling
    }

    err := s.ice.SetRemoteCandidates(sig.ICECandidates)
    if err != nil {
        return err
    }

    // Start the ICE transport
    err = s.ice.Start(nil, sig.ICEParameters, &iceRole)
    if err != nil {
        return err
    }

    // Start the DTLS transport
    err = s.dtls.Start(sig.DTLSParameters)
    if err != nil {
        return err
    }

    // Start the SCTP transport
    err = s.sctp.Start(sig.SCTPCapabilities)
    if err != nil {
        return err
    }

    return nil
}

#[tokio::test] async fn(s *testORTCStack) getSignal() (*testORTCSignal, error) {
    gatherFinished := make(chan struct{})
    s.gatherer.OnLocalCandidate(func(i *ICECandidate) {
        if i == nil {
            close(gatherFinished)
        }
    })

    if err := s.gatherer.Gather(); err != nil {
        return nil, err
    }

    <-gatherFinished
    iceCandidates, err := s.gatherer.GetLocalCandidates()
    if err != nil {
        return nil, err
    }

    iceParams, err := s.gatherer.GetLocalParameters()
    if err != nil {
        return nil, err
    }

    dtlsParams, err := s.dtls.GetLocalParameters()
    if err != nil {
        return nil, err
    }

    sctpCapabilities := s.sctp.GetCapabilities()

    return &testORTCSignal{
        ICECandidates:    iceCandidates,
        ICEParameters:    iceParams,
        DTLSParameters:   dtlsParams,
        SCTPCapabilities: sctpCapabilities,
    }, nil
}

#[tokio::test] async fn(s *testORTCStack) close() error {
    var closeErrs []error

    if err := s.sctp.Stop(); err != nil {
        closeErrs = append(closeErrs, err)
    }

    if err := s.ice.Stop(); err != nil {
        closeErrs = append(closeErrs, err)
    }

    return util.FlattenErrs(closeErrs)
}

type testORTCSignal struct {
    ICECandidates    []ICECandidate   `json:"iceCandidates"`
    ICEParameters    ICEParameters    `json:"iceParameters"`
    DTLSParameters   DTLSParameters   `json:"dtlsParameters"`
    SCTPCapabilities SCTPCapabilities `json:"sctpCapabilities"`
}

#[tokio::test] async fnnewORTCPair() (stackA *testORTCStack, stackB *testORTCStack, err error) {
    sa, err := newORTCStack()
    if err != nil {
        return nil, nil, err
    }

    sb, err := newORTCStack()
    if err != nil {
        return nil, nil, err
    }

    return sa, sb, nil
}

#[tokio::test] async fnnewORTCStack() (*testORTCStack, error) {
    // Create an API object
    api := NewAPI()

    // Create the ICE gatherer
    gatherer, err := api.NewICEGatherer(ICEGatherOptions{})
    if err != nil {
        return nil, err
    }

    // Construct the ICE transport
    ice := api.NewICETransport(gatherer)

    // Construct the DTLS transport
    dtls, err := api.NewDTLSTransport(ice, nil)
    if err != nil {
        return nil, err
    }

    // Construct the SCTP transport
    sctp := api.NewSCTPTransport(dtls)

    return &testORTCStack{
        api:      api,
        gatherer: gatherer,
        ice:      ice,
        dtls:     dtls,
        sctp:     sctp,
    }, nil
}

#[tokio::test] async fnsignalORTCPair(stackA *testORTCStack, stackB *testORTCStack) error {
    sigA, err := stackA.getSignal()
    if err != nil {
        return err
    }
    sigB, err := stackB.getSignal()
    if err != nil {
        return err
    }

    a := make(chan error)
    b := make(chan error)

    go func() {
        a <- stackB.setSignal(sigA, false)
    }()

    go func() {
        b <- stackA.setSignal(sigB, true)
    }()

    errA := <-a
    errB := <-b

    closeErrs := []error{errA, errB}

    return util.FlattenErrs(closeErrs)
}
*/
