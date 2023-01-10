use super::*;
use crate::mock::mock_stream::MockStream;
use crate::stream_info::RTPHeaderExtension;
use rtp::packet::Packet;
use tokio::sync::mpsc;
use tokio::time::Duration;
use util::Unmarshal;
use waitgroup::WaitGroup;

#[tokio::test]
async fn test_twcc_sender_interceptor() -> Result<()> {
    // "add transport wide cc to each packet"
    let builder = Sender::builder().with_init_sequence_nr(0);
    let icpr = builder.build("")?;

    let (p_chan_tx, mut p_chan_rx) = mpsc::channel::<Packet>(10 * 5);
    tokio::spawn(async move {
        // start some parallel streams using the same interceptor to test for race conditions
        let wg = WaitGroup::new();
        for i in 0..10 {
            let w = wg.worker();
            let p_chan_tx2 = p_chan_tx.clone();
            let icpr2 = Arc::clone(&icpr);
            tokio::spawn(async move {
                let _d = w;
                let stream = MockStream::new(
                    &StreamInfo {
                        rtp_header_extensions: vec![RTPHeaderExtension {
                            uri: TRANSPORT_CC_URI.to_owned(),
                            id: 1,
                        }],
                        ..Default::default()
                    },
                    icpr2,
                )
                .await;

                let id = i + 1;
                #[allow(clippy::identity_op)]
                for seq_num in [id * 1, id * 2, id * 3, id * 4, id * 5] {
                    stream
                        .write_rtp(&rtp::packet::Packet {
                            header: rtp::header::Header {
                                sequence_number: seq_num,
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .await
                        .unwrap();

                    let timeout = tokio::time::sleep(Duration::from_millis(10));
                    tokio::pin!(timeout);

                    tokio::select! {
                        p = stream.written_rtp() =>{
                            if let Some(p) = p {
                                assert_eq!(p.header.sequence_number, seq_num);
                                let _ = p_chan_tx2.send(p).await;
                            }else{
                                panic!("stream.written_rtp none");
                            }
                        }
                        _ = timeout.as_mut()=>{
                            panic!("written rtp packet not found");
                        }
                    };
                }

                let _ = stream.close().await;
            });
        }
        wg.wait().await;
    });

    while let Some(p) = p_chan_rx.recv().await {
        // Can't check for increasing transport cc sequence number, since we can't ensure ordering between the streams
        // on pChan is same as in the interceptor, but at least make sure each packet has a seq nr.
        let mut extension_header = p.header.get_extension(1).unwrap();
        let _twcc = TransportCcExtension::unmarshal(&mut extension_header)?;
    }

    Ok(())
}
