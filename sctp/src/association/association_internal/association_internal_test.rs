use super::*;
use std::io;
use std::net::SocketAddr;

type Result<T> = std::result::Result<T, util::Error>;

impl From<Error> for util::Error {
    fn from(e: Error) -> Self {
        util::Error::from_std(e)
    }
}

struct DumbConn;

#[async_trait]
impl Conn for DumbConn {
    async fn connect(&self, _addr: SocketAddr) -> Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    async fn recv(&self, _b: &mut [u8]) -> Result<usize> {
        Ok(0)
    }

    async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    async fn send(&self, _b: &[u8]) -> Result<usize> {
        Ok(0)
    }

    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> Result<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        Err(io::Error::new(io::ErrorKind::AddrNotAvailable, "Addr Not Available").into())
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        None
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}

fn create_association_internal(config: Config) -> AssociationInternal {
    let (close_loop_ch_tx, _close_loop_ch_rx) = broadcast::channel(1);
    let (accept_ch_tx, _accept_ch_rx) = mpsc::channel(1);
    let (handshake_completed_ch_tx, _handshake_completed_ch_rx) = mpsc::channel(1);
    let (awake_write_loop_ch_tx, _awake_write_loop_ch_rx) = mpsc::channel(1);
    AssociationInternal::new(
        config,
        close_loop_ch_tx,
        accept_ch_tx,
        handshake_completed_ch_tx,
        Arc::new(awake_write_loop_ch_tx),
    )
}

#[test]
fn test_create_forward_tsn_forward_one_abandoned() -> Result<()> {
    let mut a = AssociationInternal {
        cumulative_tsn_ack_point: 9,
        ..Default::default()
    };

    a.advanced_peer_tsn_ack_point = 10;
    a.inflight_queue.push_no_check(ChunkPayloadData {
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 10,
        stream_identifier: 1,
        stream_sequence_number: 2,
        user_data: Bytes::from_static(b"ABC"),
        nsent: 1,
        abandoned: Arc::new(AtomicBool::new(true)),
        ..Default::default()
    });

    let fwdtsn = a.create_forward_tsn();

    assert_eq!(fwdtsn.new_cumulative_tsn, 10, "should be able to serialize");
    assert_eq!(fwdtsn.streams.len(), 1, "there should be one stream");
    assert_eq!(fwdtsn.streams[0].identifier, 1, "si should be 1");
    assert_eq!(fwdtsn.streams[0].sequence, 2, "ssn should be 2");

    Ok(())
}

#[test]
fn test_create_forward_tsn_forward_two_abandoned_with_the_same_si() -> Result<()> {
    let mut a = AssociationInternal {
        cumulative_tsn_ack_point: 9,
        ..Default::default()
    };

    a.advanced_peer_tsn_ack_point = 12;
    a.inflight_queue.push_no_check(ChunkPayloadData {
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 10,
        stream_identifier: 1,
        stream_sequence_number: 2,
        user_data: Bytes::from_static(b"ABC"),
        nsent: 1,
        abandoned: Arc::new(AtomicBool::new(true)),
        ..Default::default()
    });
    a.inflight_queue.push_no_check(ChunkPayloadData {
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 11,
        stream_identifier: 1,
        stream_sequence_number: 3,
        user_data: Bytes::from_static(b"DEF"),
        nsent: 1,
        abandoned: Arc::new(AtomicBool::new(true)),
        ..Default::default()
    });
    a.inflight_queue.push_no_check(ChunkPayloadData {
        beginning_fragment: true,
        ending_fragment: true,
        tsn: 12,
        stream_identifier: 2,
        stream_sequence_number: 1,
        user_data: Bytes::from_static(b"123"),
        nsent: 1,
        abandoned: Arc::new(AtomicBool::new(true)),
        ..Default::default()
    });

    let fwdtsn = a.create_forward_tsn();

    assert_eq!(fwdtsn.new_cumulative_tsn, 12, "should be able to serialize");
    assert_eq!(fwdtsn.streams.len(), 2, "there should be two stream");

    let mut si1ok = false;
    let mut si2ok = false;
    for s in &fwdtsn.streams {
        match s.identifier {
            1 => {
                assert_eq!(3, s.sequence, "ssn should be 3");
                si1ok = true;
            }
            2 => {
                assert_eq!(1, s.sequence, "ssn should be 1");
                si2ok = true;
            }
            _ => panic!("unexpected stream indentifier"),
        }
    }
    assert!(si1ok, "si=1 should be present");
    assert!(si2ok, "si=2 should be present");

    Ok(())
}

#[tokio::test]
async fn test_handle_forward_tsn_forward_3unreceived_chunks() -> Result<()> {
    let mut a = AssociationInternal {
        use_forward_tsn: true,
        ..Default::default()
    };

    let prev_tsn = a.peer_last_tsn;

    let fwdtsn = ChunkForwardTsn {
        new_cumulative_tsn: a.peer_last_tsn + 3,
        streams: vec![ChunkForwardTsnStream {
            identifier: 0,
            sequence: 0,
        }],
    };

    let p = a.handle_forward_tsn(&fwdtsn).await?;

    let delayed_ack_triggered = a.delayed_ack_triggered;
    let immediate_ack_triggered = a.immediate_ack_triggered;
    assert_eq!(
        a.peer_last_tsn,
        prev_tsn + 3,
        "peerLastTSN should advance by 3 "
    );
    assert!(delayed_ack_triggered, "delayed sack should be triggered");
    assert!(
        !immediate_ack_triggered,
        "immediate sack should NOT be triggered"
    );
    assert!(p.is_empty(), "should return empty");

    Ok(())
}

#[tokio::test]
async fn test_handle_forward_tsn_forward_1for1_missing() -> Result<()> {
    let mut a = AssociationInternal {
        use_forward_tsn: true,
        ..Default::default()
    };

    let prev_tsn = a.peer_last_tsn;

    // this chunk is blocked by the missing chunk at tsn=1
    a.payload_queue.push(
        ChunkPayloadData {
            beginning_fragment: true,
            ending_fragment: true,
            tsn: a.peer_last_tsn + 2,
            stream_identifier: 0,
            stream_sequence_number: 1,
            user_data: Bytes::from_static(b"ABC"),
            ..Default::default()
        },
        a.peer_last_tsn,
    );

    let fwdtsn = ChunkForwardTsn {
        new_cumulative_tsn: a.peer_last_tsn + 1,
        streams: vec![ChunkForwardTsnStream {
            identifier: 0,
            sequence: 1,
        }],
    };

    let p = a.handle_forward_tsn(&fwdtsn).await?;

    let delayed_ack_triggered = a.delayed_ack_triggered;
    let immediate_ack_triggered = a.immediate_ack_triggered;
    assert_eq!(
        a.peer_last_tsn,
        prev_tsn + 2,
        "peerLastTSN should advance by 2"
    );
    assert!(delayed_ack_triggered, "delayed sack should be triggered");
    assert!(
        !immediate_ack_triggered,
        "immediate sack should NOT be triggered"
    );
    assert!(p.is_empty(), "should return empty");

    Ok(())
}

#[tokio::test]
async fn test_handle_forward_tsn_forward_1for2_missing() -> Result<()> {
    let mut a = AssociationInternal {
        use_forward_tsn: true,
        ..Default::default()
    };

    let prev_tsn = a.peer_last_tsn;

    // this chunk is blocked by the missing chunk at tsn=1
    a.payload_queue.push(
        ChunkPayloadData {
            beginning_fragment: true,
            ending_fragment: true,
            tsn: a.peer_last_tsn + 3,
            stream_identifier: 0,
            stream_sequence_number: 1,
            user_data: Bytes::from_static(b"ABC"),
            ..Default::default()
        },
        a.peer_last_tsn,
    );

    let fwdtsn = ChunkForwardTsn {
        new_cumulative_tsn: a.peer_last_tsn + 1,
        streams: vec![ChunkForwardTsnStream {
            identifier: 0,
            sequence: 1,
        }],
    };

    let p = a.handle_forward_tsn(&fwdtsn).await?;

    let immediate_ack_triggered = a.immediate_ack_triggered;
    assert_eq!(
        a.peer_last_tsn,
        prev_tsn + 1,
        "peerLastTSN should advance by 1"
    );
    assert!(
        immediate_ack_triggered,
        "immediate sack should be triggered"
    );
    assert!(p.is_empty(), "should return empty");

    Ok(())
}

#[tokio::test]
async fn test_handle_forward_tsn_dup_forward_tsn_chunk_should_generate_sack() -> Result<()> {
    let mut a = AssociationInternal {
        use_forward_tsn: true,
        ..Default::default()
    };

    let prev_tsn = a.peer_last_tsn;

    let fwdtsn = ChunkForwardTsn {
        new_cumulative_tsn: a.peer_last_tsn,
        streams: vec![ChunkForwardTsnStream {
            identifier: 0,
            sequence: 1,
        }],
    };

    let p = a.handle_forward_tsn(&fwdtsn).await?;

    assert_eq!(a.peer_last_tsn, prev_tsn, "peerLastTSN should not advance");
    assert_eq!(a.ack_state, AckState::Immediate, "sack should be requested");
    assert!(p.is_empty(), "should return empty");

    Ok(())
}

#[tokio::test]
async fn test_assoc_create_new_stream() -> Result<()> {
    let (accept_ch_tx, _accept_ch_rx) = mpsc::channel(ACCEPT_CH_SIZE);
    let mut a = AssociationInternal {
        accept_ch_tx: Some(accept_ch_tx),
        ..Default::default()
    };

    for i in 0..ACCEPT_CH_SIZE {
        let s = a.create_stream(i as u16, true);
        if let Some(s) = s {
            let result = a.streams.get(&s.stream_identifier);
            assert!(result.is_some(), "should be in a.streams map");
        } else {
            panic!("{i} should success");
        }
    }

    let new_si = ACCEPT_CH_SIZE as u16;
    let s = a.create_stream(new_si, true);
    assert!(s.is_none(), "should be none");
    let result = a.streams.get(&new_si);
    assert!(result.is_none(), "should NOT be in a.streams map");

    let to_be_ignored = ChunkPayloadData {
        beginning_fragment: true,
        ending_fragment: true,
        tsn: a.peer_last_tsn + 1,
        stream_identifier: new_si,
        user_data: Bytes::from_static(b"ABC"),
        ..Default::default()
    };

    let p = a.handle_data(&to_be_ignored).await?;
    assert!(p.is_empty(), "should return empty");

    Ok(())
}

async fn handle_init_test(name: &str, initial_state: AssociationState, expect_err: bool) {
    let mut a = create_association_internal(Config {
        net_conn: Arc::new(DumbConn {}),
        max_receive_buffer_size: 0,
        max_message_size: 0,
        name: "client".to_owned(),
    });
    a.set_state(initial_state);
    let pkt = Packet {
        source_port: 5001,
        destination_port: 5002,
        ..Default::default()
    };
    let mut init = ChunkInit {
        initial_tsn: 1234,
        num_outbound_streams: 1001,
        num_inbound_streams: 1002,
        initiate_tag: 5678,
        advertised_receiver_window_credit: 512 * 1024,
        ..Default::default()
    };
    init.set_supported_extensions();

    let result = a.handle_init(&pkt, &init).await;
    if expect_err {
        assert!(result.is_err(), "{name} should fail");
        return;
    } else {
        assert!(result.is_ok(), "{name} should be ok");
    }
    assert_eq!(
        a.peer_last_tsn,
        if init.initial_tsn == 0 {
            u32::MAX
        } else {
            init.initial_tsn - 1
        },
        "{name} should match"
    );
    assert_eq!(a.my_max_num_outbound_streams, 1001, "{name} should match");
    assert_eq!(a.my_max_num_inbound_streams, 1002, "{name} should match");
    assert_eq!(a.peer_verification_tag, 5678, "{name} should match");
    assert_eq!(a.destination_port, pkt.source_port, "{name} should match");
    assert_eq!(a.source_port, pkt.destination_port, "{name} should match");
    assert!(a.use_forward_tsn, "{name} should be set to true");
}

#[tokio::test]
async fn test_assoc_handle_init() -> Result<()> {
    handle_init_test("normal", AssociationState::Closed, false).await;

    handle_init_test(
        "unexpected state established",
        AssociationState::Established,
        true,
    )
    .await;

    handle_init_test(
        "unexpected state shutdownAckSent",
        AssociationState::ShutdownAckSent,
        true,
    )
    .await;

    handle_init_test(
        "unexpected state shutdownPending",
        AssociationState::ShutdownPending,
        true,
    )
    .await;

    handle_init_test(
        "unexpected state shutdownReceived",
        AssociationState::ShutdownReceived,
        true,
    )
    .await;

    handle_init_test(
        "unexpected state shutdownSent",
        AssociationState::ShutdownSent,
        true,
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn test_assoc_max_message_size_default() -> Result<()> {
    let mut a = create_association_internal(Config {
        net_conn: Arc::new(DumbConn {}),
        max_receive_buffer_size: 0,
        max_message_size: 0,
        name: "client".to_owned(),
    });
    assert_eq!(
        a.max_message_size.load(Ordering::SeqCst),
        65536,
        "should match"
    );

    let stream = a.create_stream(1, false);
    assert!(stream.is_some(), "should succeed");

    if let Some(s) = stream {
        let p = Bytes::from(vec![0u8; 65537]);
        let ppi = PayloadProtocolIdentifier::from(s.default_payload_type.load(Ordering::SeqCst));

        if let Err(err) = s.write_sctp(&p.slice(..65536), ppi).await {
            assert_ne!(
                err,
                Error::ErrOutboundPacketTooLarge,
                "should be not Error::ErrOutboundPacketTooLarge"
            );
        } else {
            panic!("should be error");
        }

        if let Err(err) = s.write_sctp(&p.slice(..65537), ppi).await {
            assert_eq!(
                err,
                Error::ErrOutboundPacketTooLarge,
                "should be Error::ErrOutboundPacketTooLarge"
            );
        } else {
            panic!("should be error");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_assoc_max_message_size_explicit() -> Result<()> {
    let mut a = create_association_internal(Config {
        net_conn: Arc::new(DumbConn {}),
        max_receive_buffer_size: 0,
        max_message_size: 30000,
        name: "client".to_owned(),
    });

    assert_eq!(
        a.max_message_size.load(Ordering::SeqCst),
        30000,
        "should match"
    );

    let stream = a.create_stream(1, false);
    assert!(stream.is_some(), "should succeed");

    if let Some(s) = stream {
        let p = Bytes::from(vec![0u8; 30001]);
        let ppi = PayloadProtocolIdentifier::from(s.default_payload_type.load(Ordering::SeqCst));

        if let Err(err) = s.write_sctp(&p.slice(..30000), ppi).await {
            assert_ne!(
                err,
                Error::ErrOutboundPacketTooLarge,
                "should be not Error::ErrOutboundPacketTooLarge"
            );
        } else {
            panic!("should be error");
        }

        if let Err(err) = s.write_sctp(&p.slice(..30001), ppi).await {
            assert_eq!(
                err,
                Error::ErrOutboundPacketTooLarge,
                "should be Error::ErrOutboundPacketTooLarge"
            );
        } else {
            panic!("should be error");
        }
    }

    Ok(())
}
