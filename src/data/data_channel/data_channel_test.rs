use super::*;
use crate::api::{APIBuilder, API};
use crate::data::data_channel::data_channel_config::DataChannelConfig;
use crate::peer::peer_connection::peer_connection_test::*;
use crate::peer::peer_connection::PeerConnection;

use tokio::sync::mpsc;

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

//TODO: finish test_data_channel
use crate::api::media_engine::MediaEngine;
use log::LevelFilter;
use std::io::Write;
use tokio::time::Duration;

#[tokio::test]
async fn test_data_channel_open() -> Result<()> {
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
        .filter(None, LevelFilter::Trace)
        .init();

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

/*
#[tokio::test]
async fn  TestDataChannel_Send()->Result<()> {
    t.Run("before signaling", func()->Result<()> {
        report := test.CheckRoutines(t)
        defer report()

        offerPC, answerPC, err := newPair()
        if err != nil {
            t.Fatalf("Failed to create a PC pair for testing")
        }

        done := make(chan bool)

        answerPC.OnDataChannel(func(d *DataChannel) {
            // Make sure this is the data channel we were looking for. (Not the one
            // created in signalPair).
            if d.Label() != EXPECTED_LABEL {
                return
            }
            d.OnMessage(func(msg DataChannelMessage) {
                e := d.Send([]byte("Pong"))
                if e != nil {
                    t.Fatalf("Failed to send string on data channel")
                }
            })
            assert.True(t, d.Ordered(), "Ordered should be set to true")
        })

        dc, err := offerPC.CreateDataChannel(EXPECTED_LABEL, nil)
        if err != nil {
            t.Fatalf("Failed to create a PC pair for testing")
        }

        assert.True(t, dc.Ordered(), "Ordered should be set to true")

        dc.OnOpen(func() {
            e := dc.SendText("Ping")
            if e != nil {
                t.Fatalf("Failed to send string on data channel")
            }
        })
        dc.OnMessage(func(msg DataChannelMessage) {
            done <- true
        })

        err = signalPair(offerPC, answerPC)
        if err != nil {
            t.Fatalf("Failed to signal our PC pair for testing: %+v", err)
        }

        close_pair(t, offerPC, answerPC, done)
    })

    t.Run("after connected", func()->Result<()> {
        report := test.CheckRoutines(t)
        defer report()

        offerPC, answerPC, err := newPair()
        if err != nil {
            t.Fatalf("Failed to create a PC pair for testing")
        }

        done := make(chan bool)

        answerPC.OnDataChannel(func(d *DataChannel) {
            // Make sure this is the data channel we were looking for. (Not the one
            // created in signalPair).
            if d.Label() != EXPECTED_LABEL {
                return
            }
            d.OnMessage(func(msg DataChannelMessage) {
                e := d.Send([]byte("Pong"))
                if e != nil {
                    t.Fatalf("Failed to send string on data channel")
                }
            })
            assert.True(t, d.Ordered(), "Ordered should be set to true")
        })

        once := &sync.Once{}
        offerPC.OnICEConnectionStateChange(func(state ICEConnectionState) {
            if state == ICEConnectionStateConnected || state == ICEConnectionStateCompleted {
                // wasm fires completed state multiple times
                once.Do(func() {
                    dc, createErr := offerPC.CreateDataChannel(EXPECTED_LABEL, nil)
                    if createErr != nil {
                        t.Fatalf("Failed to create a PC pair for testing")
                    }

                    assert.True(t, dc.Ordered(), "Ordered should be set to true")

                    dc.OnMessage(func(msg DataChannelMessage) {
                        done <- true
                    })

                    if e := dc.SendText("Ping"); e != nil {
                        // wasm binding doesn't fire OnOpen (we probably already missed it)
                        dc.OnOpen(func() {
                            e = dc.SendText("Ping")
                            if e != nil {
                                t.Fatalf("Failed to send string on data channel")
                            }
                        })
                    }
                })
            }
        })

        err = signalPair(offerPC, answerPC)
        if err != nil {
            t.Fatalf("Failed to signal our PC pair for testing")
        }

        close_pair(t, offerPC, answerPC, done)
    })
}

#[tokio::test]
async fn  TestDataChannel_Close()->Result<()> {
    report := test.CheckRoutines(t)
    defer report()

    t.Run("Close after PeerConnection Closed", func()->Result<()> {
        offerPC, answerPC, err := newPair()
        assert.NoError(t, err)

        dc, err := offerPC.CreateDataChannel(EXPECTED_LABEL, nil)
        assert.NoError(t, err)

        close_pair_now(t, offerPC, answerPC)
        assert.NoError(t, dc.Close())
    })

    t.Run("Close before connected", func()->Result<()> {
        offerPC, answerPC, err := newPair()
        assert.NoError(t, err)

        dc, err := offerPC.CreateDataChannel(EXPECTED_LABEL, nil)
        assert.NoError(t, err)

        assert.NoError(t, dc.Close())
        close_pair_now(t, offerPC, answerPC)
    })
}

#[tokio::test]
async fn  TestDataChannelParameters()->Result<()> {
    report := test.CheckRoutines(t)
    defer report()

    t.Run("MaxPacketLifeTime exchange", func()->Result<()> {
        ordered := true
        maxPacketLifeTime := uint16(3)
        options := &DataChannelInit{
            Ordered:           &ordered,
            MaxPacketLifeTime: &maxPacketLifeTime,
        }

        offerPC, answerPC, dc, done := set_up_data_channel_parameters_test(t, options)

        // Check if parameters are correctly set
        assert.Equal(t, dc.Ordered(), ordered, "Ordered should be same value as set in DataChannelInit")
        if assert.NotNil(t, dc.MaxPacketLifeTime(), "should not be nil") {
            assert.Equal(t, maxPacketLifeTime, *dc.MaxPacketLifeTime(), "should match")
        }

        answerPC.OnDataChannel(func(d *DataChannel) {
            if d.Label() != EXPECTED_LABEL {
                return
            }
            // Check if parameters are correctly set
            assert.Equal(t, d.Ordered(), ordered, "Ordered should be same value as set in DataChannelInit")
            if assert.NotNil(t, d.MaxPacketLifeTime(), "should not be nil") {
                assert.Equal(t, maxPacketLifeTime, *d.MaxPacketLifeTime(), "should match")
            }
            done <- true
        })

        close_reliability_param_test(t, offerPC, answerPC, done)
    })

    t.Run("MaxRetransmits exchange", func()->Result<()> {
        ordered := false
        maxRetransmits := uint16(3000)
        options := &DataChannelInit{
            Ordered:        &ordered,
            MaxRetransmits: &maxRetransmits,
        }

        offerPC, answerPC, dc, done := set_up_data_channel_parameters_test(t, options)

        // Check if parameters are correctly set
        assert.False(t, dc.Ordered(), "Ordered should be set to false")
        if assert.NotNil(t, dc.MaxRetransmits(), "should not be nil") {
            assert.Equal(t, maxRetransmits, *dc.MaxRetransmits(), "should match")
        }

        answerPC.OnDataChannel(func(d *DataChannel) {
            // Make sure this is the data channel we were looking for. (Not the one
            // created in signalPair).
            if d.Label() != EXPECTED_LABEL {
                return
            }
            // Check if parameters are correctly set
            assert.False(t, d.Ordered(), "Ordered should be set to false")
            if assert.NotNil(t, d.MaxRetransmits(), "should not be nil") {
                assert.Equal(t, maxRetransmits, *d.MaxRetransmits(), "should match")
            }
            done <- true
        })

        close_reliability_param_test(t, offerPC, answerPC, done)
    })

    t.Run("Protocol exchange", func()->Result<()> {
        protocol := "json"
        options := &DataChannelInit{
            Protocol: &protocol,
        }

        offerPC, answerPC, dc, done := set_up_data_channel_parameters_test(t, options)

        // Check if parameters are correctly set
        assert.Equal(t, protocol, dc.Protocol(), "Protocol should match DataChannelInit")

        answerPC.OnDataChannel(func(d *DataChannel) {
            // Make sure this is the data channel we were looking for. (Not the one
            // created in signalPair).
            if d.Label() != EXPECTED_LABEL {
                return
            }
            // Check if parameters are correctly set
            assert.Equal(t, protocol, d.Protocol(), "Protocol should match what channel creator declared")
            done <- true
        })

        close_reliability_param_test(t, offerPC, answerPC, done)
    })

    t.Run("Negotiated exchange", func()->Result<()> {
        const expectedMessage = "Hello World"

        negotiated := true
        var id uint16 = 500
        options := &DataChannelInit{
            Negotiated: &negotiated,
            ID:         &id,
        }

        offerPC, answerPC, offerDatachannel, done := set_up_data_channel_parameters_test(t, options)
        answerDatachannel, err := answerPC.CreateDataChannel(EXPECTED_LABEL, options)
        assert.NoError(t, err)

        answerPC.OnDataChannel(func(d *DataChannel) {
            // Ignore our default channel, exists to force ICE candidates. See signalPair for more info
            if d.Label() == "initial_data_channel" {
                return
            }

            t.Fatal("OnDataChannel must not be fired when negotiated == true")
        })
        offerPC.OnDataChannel(func(d *DataChannel) {
            t.Fatal("OnDataChannel must not be fired when negotiated == true")
        })

        seenAnswerMessage := &atomicBool{}
        seenOfferMessage := &atomicBool{}

        answerDatachannel.OnMessage(func(msg DataChannelMessage) {
            if msg.IsString && string(msg.Data) == expectedMessage {
                seenAnswerMessage.set(true)
            }
        })

        offerDatachannel.OnMessage(func(msg DataChannelMessage) {
            if msg.IsString && string(msg.Data) == expectedMessage {
                seenOfferMessage.set(true)
            }
        })

        go func() {
            for {
                if seenAnswerMessage.get() && seenOfferMessage.get() {
                    break
                }

                if offerDatachannel.ReadyState() == DataChannelStateOpen {
                    assert.NoError(t, offerDatachannel.SendText(expectedMessage))
                }
                if answerDatachannel.ReadyState() == DataChannelStateOpen {
                    assert.NoError(t, answerDatachannel.SendText(expectedMessage))
                }

                time.Sleep(500 * time.Millisecond)
            }

            done <- true
        }()

        close_reliability_param_test(t, offerPC, answerPC, done)
    })
}
*/
