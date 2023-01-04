// Silence warning on `..Default::default()` with no effect:
#![allow(clippy::needless_update)]

use super::*;
use crate::mock::mock_stream::MockStream;
use crate::stream_info::RTPHeaderExtension;
use rtcp::transport_feedbacks::transport_layer_cc::{
    PacketStatusChunk, RunLengthChunk, StatusChunkTypeTcc, StatusVectorChunk, SymbolSizeTypeTcc,
    SymbolTypeTcc, TransportLayerCc,
};
use util::Marshal;

#[tokio::test]
async fn test_twcc_receiver_interceptor_before_any_packets() -> Result<()> {
    let builder = Receiver::builder();
    let icpr = builder.build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 1,
            rtp_header_extensions: vec![RTPHeaderExtension {
                uri: TRANSPORT_CC_URI.to_owned(),
                id: 1,
                ..Default::default()
            }],
            ..Default::default()
        },
        icpr,
    )
    .await;

    tokio::select! {
        pkts = stream.written_rtcp() => {
            assert!(pkts.map(|p| p.is_empty()).unwrap_or(true), "Should not have sent an RTCP packet before receiving the first RTP packets")
        }
        _ = tokio::time::sleep(Duration::from_millis(300)) => {
            // All good
        }
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_twcc_receiver_interceptor_after_rtp_packets() -> Result<()> {
    let builder = Receiver::builder();
    let icpr = builder.build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 1,
            rtp_header_extensions: vec![RTPHeaderExtension {
                uri: TRANSPORT_CC_URI.to_owned(),
                id: 1,
                ..Default::default()
            }],
            ..Default::default()
        },
        icpr,
    )
    .await;

    for i in 0..10 {
        let mut hdr = rtp::header::Header::default();
        let tcc = TransportCcExtension {
            transport_sequence: i,
        }
        .marshal()?;
        hdr.set_extension(1, tcc)?;
        stream
            .receive_rtp(rtp::packet::Packet {
                header: hdr,
                ..Default::default()
            })
            .await;
    }

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(cc) = pkts[0].as_any().downcast_ref::<TransportLayerCc>() {
        assert_eq!(cc.media_ssrc, 1);
        assert_eq!(cc.base_sequence_number, 0);
        assert_eq!(
            cc.packet_chunks,
            vec![PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                type_tcc: StatusChunkTypeTcc::RunLengthChunk,
                packet_status_symbol: SymbolTypeTcc::PacketReceivedSmallDelta,
                run_length: 10,
            })]
        );
    } else {
        panic!();
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn test_twcc_receiver_interceptor_different_delays_between_rtp_packets() -> Result<()> {
    let builder = Receiver::builder().with_interval(Duration::from_millis(500));
    let icpr = builder.build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 1,
            rtp_header_extensions: vec![RTPHeaderExtension {
                uri: TRANSPORT_CC_URI.to_owned(),
                id: 1,
                ..Default::default()
            }],
            ..Default::default()
        },
        icpr,
    )
    .await;

    let delays = vec![0, 10, 100, 200];
    for (i, d) in delays.iter().enumerate() {
        tokio::time::advance(Duration::from_millis(*d)).await;

        let mut hdr = rtp::header::Header::default();
        let tcc = TransportCcExtension {
            transport_sequence: i as u16,
        }
        .marshal()?;

        hdr.set_extension(1, tcc)?;
        stream
            .receive_rtp(rtp::packet::Packet {
                header: hdr,
                ..Default::default()
            })
            .await;

        // Yield so this packet can be processed
        tokio::task::yield_now().await;
    }

    // Force a packet to be generated
    tokio::time::advance(Duration::from_millis(2001)).await;
    tokio::task::yield_now().await;

    let pkts = stream.written_rtcp().await.unwrap();

    assert_eq!(pkts.len(), 1);
    if let Some(cc) = pkts[0].as_any().downcast_ref::<TransportLayerCc>() {
        assert_eq!(cc.base_sequence_number, 0);
        assert_eq!(
            cc.packet_chunks,
            vec![PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
                symbol_size: SymbolSizeTypeTcc::TwoBit,
                symbol_list: vec![
                    SymbolTypeTcc::PacketReceivedSmallDelta,
                    SymbolTypeTcc::PacketReceivedSmallDelta,
                    SymbolTypeTcc::PacketReceivedLargeDelta,
                    SymbolTypeTcc::PacketReceivedLargeDelta,
                ],
            })]
        );
    } else {
        panic!();
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn test_twcc_receiver_interceptor_packet_loss() -> Result<()> {
    let builder = Receiver::builder().with_interval(Duration::from_secs(2));
    let icpr = builder.build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 1,
            rtp_header_extensions: vec![RTPHeaderExtension {
                uri: TRANSPORT_CC_URI.to_owned(),
                id: 1,
                ..Default::default()
            }],
            ..Default::default()
        },
        icpr,
    )
    .await;

    let sequence_number_to_delay = &[
        (0, 0),
        (1, 10),
        (4, 100),
        (8, 200),
        (9, 20),
        (10, 20),
        (30, 300),
    ];

    for (i, d) in sequence_number_to_delay {
        tokio::time::advance(Duration::from_millis(*d)).await;
        let mut hdr = rtp::header::Header::default();
        let tcc = TransportCcExtension {
            transport_sequence: *i,
        }
        .marshal()?;
        hdr.set_extension(1, tcc)?;
        stream
            .receive_rtp(rtp::packet::Packet {
                header: hdr,
                ..Default::default()
            })
            .await;

        // Yield so this packet can be processed
        tokio::task::yield_now().await;
    }

    // Force a packet to be generated
    tokio::time::advance(Duration::from_millis(2001)).await;
    tokio::task::yield_now().await;

    let pkts = stream.written_rtcp().await.unwrap();

    assert_eq!(pkts.len(), 1);
    if let Some(cc) = pkts[0].as_any().downcast_ref::<TransportLayerCc>() {
        assert_eq!(cc.base_sequence_number, 0);
        assert_eq!(
            cc.packet_chunks,
            vec![
                PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                    type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
                    symbol_size: SymbolSizeTypeTcc::TwoBit,
                    symbol_list: vec![
                        SymbolTypeTcc::PacketReceivedSmallDelta,
                        SymbolTypeTcc::PacketReceivedSmallDelta,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketReceivedLargeDelta,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                    ],
                }),
                PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                    type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
                    symbol_size: SymbolSizeTypeTcc::TwoBit,
                    symbol_list: vec![
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketReceivedLargeDelta,
                        SymbolTypeTcc::PacketReceivedSmallDelta,
                        SymbolTypeTcc::PacketReceivedSmallDelta,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                    ],
                }),
                PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                    type_tcc: StatusChunkTypeTcc::RunLengthChunk,
                    packet_status_symbol: SymbolTypeTcc::PacketNotReceived,
                    run_length: 16,
                }),
                PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                    type_tcc: StatusChunkTypeTcc::RunLengthChunk,
                    packet_status_symbol: SymbolTypeTcc::PacketReceivedLargeDelta,
                    run_length: 1,
                }),
            ]
        );
    } else {
        panic!();
    }

    stream.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_twcc_receiver_interceptor_overflow() -> Result<()> {
    let builder = Receiver::builder();
    let icpr = builder.build("")?;

    let stream = MockStream::new(
        &StreamInfo {
            ssrc: 1,
            rtp_header_extensions: vec![RTPHeaderExtension {
                uri: TRANSPORT_CC_URI.to_owned(),
                id: 1,
                ..Default::default()
            }],
            ..Default::default()
        },
        icpr,
    )
    .await;

    for i in [65530, 65534, 65535, 1, 2, 10] {
        let mut hdr = rtp::header::Header::default();
        let tcc = TransportCcExtension {
            transport_sequence: i,
        }
        .marshal()?;
        hdr.set_extension(1, tcc)?;
        stream
            .receive_rtp(rtp::packet::Packet {
                header: hdr,
                ..Default::default()
            })
            .await;
    }

    let pkts = stream.written_rtcp().await.unwrap();
    assert_eq!(pkts.len(), 1);
    if let Some(cc) = pkts[0].as_any().downcast_ref::<TransportLayerCc>() {
        assert_eq!(cc.base_sequence_number, 65530);
        assert_eq!(
            cc.packet_chunks,
            vec![
                PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                    type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
                    symbol_size: SymbolSizeTypeTcc::OneBit,
                    symbol_list: vec![
                        SymbolTypeTcc::PacketReceivedSmallDelta,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketReceivedSmallDelta,
                        SymbolTypeTcc::PacketReceivedSmallDelta,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketReceivedSmallDelta,
                        SymbolTypeTcc::PacketReceivedSmallDelta,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                    ],
                }),
                PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                    type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
                    symbol_size: SymbolSizeTypeTcc::TwoBit,
                    symbol_list: vec![
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketNotReceived,
                        SymbolTypeTcc::PacketReceivedSmallDelta,
                    ],
                }),
            ]
        );
    } else {
        panic!();
    }

    stream.close().await?;

    Ok(())
}
