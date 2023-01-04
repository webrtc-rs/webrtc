use crate::error::Result;

use super::*;

use util::conn::conn_bridge::*;
use util::conn::*;

use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::{broadcast, mpsc};
use tokio::time::Duration;

async fn bridge_process_at_least_one(br: &Arc<Bridge>) {
    let mut n_sum = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(10)).await;
        n_sum += br.tick().await;
        if br.len(0).await == 0 && br.len(1).await == 0 && n_sum > 0 {
            break;
        }
    }
}

async fn create_new_association_pair(
    br: &Arc<Bridge>,
    ca: Arc<dyn Conn + Send + Sync>,
    cb: Arc<dyn Conn + Send + Sync>,
) -> Result<(Arc<Association>, Arc<Association>)> {
    let (handshake0ch_tx, mut handshake0ch_rx) = mpsc::channel(1);
    let (handshake1ch_tx, mut handshake1ch_rx) = mpsc::channel(1);
    let (closed_tx, mut closed_rx0) = broadcast::channel::<()>(1);
    let mut closed_rx1 = closed_tx.subscribe();

    // Setup client
    tokio::spawn(async move {
        let client = Association::client(sctp::association::Config {
            net_conn: ca,
            max_receive_buffer_size: 0,
            max_message_size: 0,
            name: "client".to_owned(),
        })
        .await;

        let _ = handshake0ch_tx.send(client).await;
        let _ = closed_rx0.recv().await;

        Result::<()>::Ok(())
    });

    // Setup server
    tokio::spawn(async move {
        let server = Association::server(sctp::association::Config {
            net_conn: cb,
            max_receive_buffer_size: 0,
            max_message_size: 0,
            name: "server".to_owned(),
        })
        .await;

        let _ = handshake1ch_tx.send(server).await;
        let _ = closed_rx1.recv().await;

        Result::<()>::Ok(())
    });

    let mut client = None;
    let mut server = None;
    let mut a0handshake_done = false;
    let mut a1handshake_done = false;
    let mut i = 0;
    while (!a0handshake_done || !a1handshake_done) && i < 100 {
        br.tick().await;

        let timer = tokio::time::sleep(Duration::from_millis(10));
        tokio::pin!(timer);

        tokio::select! {
            _ = timer.as_mut() =>{},
            r0 = handshake0ch_rx.recv() => {
                if let Ok(c) = r0.unwrap() {
                    client = Some(c);
                }
                a0handshake_done = true;
            },
            r1 = handshake1ch_rx.recv() => {
                if let Ok(s) = r1.unwrap() {
                    server = Some(s);
                }
                a1handshake_done = true;
            },
        };
        i += 1;
    }

    if !a0handshake_done || !a1handshake_done {
        return Err(Error::new("handshake failed".to_owned()));
    }

    drop(closed_tx);

    Ok((Arc::new(client.unwrap()), Arc::new(server.unwrap())))
}

async fn close_association_pair(
    br: &Arc<Bridge>,
    client: Arc<Association>,
    server: Arc<Association>,
) {
    let (handshake0ch_tx, mut handshake0ch_rx) = mpsc::channel(1);
    let (handshake1ch_tx, mut handshake1ch_rx) = mpsc::channel(1);
    let (closed_tx, mut closed_rx0) = broadcast::channel::<()>(1);
    let mut closed_rx1 = closed_tx.subscribe();

    // Close client
    tokio::spawn(async move {
        client.close().await?;
        let _ = handshake0ch_tx.send(()).await;
        let _ = closed_rx0.recv().await;

        Result::<()>::Ok(())
    });

    // Close server
    tokio::spawn(async move {
        server.close().await?;
        let _ = handshake1ch_tx.send(()).await;
        let _ = closed_rx1.recv().await;

        Result::<()>::Ok(())
    });

    let mut a0handshake_done = false;
    let mut a1handshake_done = false;
    let mut i = 0;
    while (!a0handshake_done || !a1handshake_done) && i < 100 {
        br.tick().await;

        let timer = tokio::time::sleep(Duration::from_millis(10));
        tokio::pin!(timer);

        tokio::select! {
            _ = timer.as_mut() =>{},
            _ = handshake0ch_rx.recv() => {
                a0handshake_done = true;
            },
            _ = handshake1ch_rx.recv() => {
                a1handshake_done = true;
            },
        };
        i += 1;
    }

    drop(closed_tx);
}

//use std::io::Write;

async fn pr_ordered_unordered_test(channel_type: ChannelType, is_ordered: bool) -> Result<()> {
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
    .filter(None, log::LevelFilter::Trace)
    .init();*/

    let mut sbuf = vec![0u8; 1000];
    let mut rbuf = vec![0u8; 2000];

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, a1) = create_new_association_pair(&br, Arc::new(ca), Arc::new(cb)).await?;

    let cfg = Config {
        channel_type,
        reliability_parameter: 0,
        label: "data".to_string(),
        ..Default::default()
    };

    let dc0 = DataChannel::dial(&a0, 100, cfg.clone()).await?;
    bridge_process_at_least_one(&br).await;

    let existing_data_channels: Vec<DataChannel> = Vec::new();
    let dc1 = DataChannel::accept(&a1, Config::default(), &existing_data_channels).await?;
    bridge_process_at_least_one(&br).await;

    assert_eq!(dc0.config, cfg, "local config should match");
    assert_eq!(dc1.config, cfg, "remote config should match");

    dc0.commit_reliability_params();
    dc1.commit_reliability_params();

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = dc0
        .write_data_channel(&Bytes::from(sbuf.clone()), true)
        .await?;
    assert_eq!(sbuf.len(), n, "data length should match");

    sbuf[0..4].copy_from_slice(&2u32.to_be_bytes());
    let n = dc0
        .write_data_channel(&Bytes::from(sbuf.clone()), true)
        .await?;
    assert_eq!(sbuf.len(), n, "data length should match");

    if !is_ordered {
        sbuf[0..4].copy_from_slice(&3u32.to_be_bytes());
        let n = dc0
            .write_data_channel(&Bytes::from(sbuf.clone()), true)
            .await?;
        assert_eq!(sbuf.len(), n, "data length should match");
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
    br.drop_offset(0, 0, 1).await; // drop the first packet on the wire
    if !is_ordered {
        br.reorder(0).await;
    } else {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bridge_process_at_least_one(&br).await;

    if !is_ordered {
        let (n, is_string) = dc1.read_data_channel(&mut rbuf[..]).await?;
        assert!(is_string, "should return isString being true");
        assert_eq!(sbuf.len(), n, "data length should match");
        assert_eq!(
            3,
            u32::from_be_bytes([rbuf[0], rbuf[1], rbuf[2], rbuf[3]]),
            "data should match"
        );
    }

    let (n, is_string) = dc1.read_data_channel(&mut rbuf[..]).await?;
    assert!(is_string, "should return isString being true");
    assert_eq!(sbuf.len(), n, "data length should match");
    assert_eq!(
        2,
        u32::from_be_bytes([rbuf[0], rbuf[1], rbuf[2], rbuf[3]]),
        "data should match"
    );

    dc0.close().await?;
    dc1.close().await?;
    bridge_process_at_least_one(&br).await;

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

#[tokio::test]
async fn test_data_channel_channel_type_reliable_ordered() -> Result<()> {
    let mut sbuf = vec![0u8; 1000];
    let mut rbuf = vec![0u8; 1500];

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, a1) = create_new_association_pair(&br, Arc::new(ca), Arc::new(cb)).await?;

    let cfg = Config {
        channel_type: ChannelType::Reliable,
        reliability_parameter: 123,
        label: "data".to_string(),
        ..Default::default()
    };

    let dc0 = DataChannel::dial(&a0, 100, cfg.clone()).await?;
    bridge_process_at_least_one(&br).await;

    let existing_data_channels: Vec<DataChannel> = Vec::new();
    let dc1 = DataChannel::accept(&a1, Config::default(), &existing_data_channels).await?;
    bridge_process_at_least_one(&br).await;

    assert_eq!(dc0.config, cfg, "local config should match");
    assert_eq!(dc1.config, cfg, "remote config should match");

    br.reorder_next_nwrites(0, 2); // reordering on the wire

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = dc0.write(&Bytes::from(sbuf.clone())).await?;
    assert_eq!(sbuf.len(), n, "data length should match");

    sbuf[0..4].copy_from_slice(&2u32.to_be_bytes());
    let n = dc0.write(&Bytes::from(sbuf.clone())).await?;
    assert_eq!(sbuf.len(), n, "data length should match");

    bridge_process_at_least_one(&br).await;

    let n = dc1.read(&mut rbuf[..]).await?;
    assert_eq!(sbuf.len(), n, "data length should match");
    assert_eq!(
        1,
        u32::from_be_bytes([rbuf[0], rbuf[1], rbuf[2], rbuf[3]]),
        "data should match"
    );

    let n = dc1.read(&mut rbuf[..]).await?;
    assert_eq!(sbuf.len(), n, "data length should match");
    assert_eq!(
        2,
        u32::from_be_bytes([rbuf[0], rbuf[1], rbuf[2], rbuf[3]]),
        "data should match"
    );

    dc0.close().await?;
    dc1.close().await?;
    bridge_process_at_least_one(&br).await;

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

#[tokio::test]
async fn test_data_channel_channel_type_reliable_unordered() -> Result<()> {
    let mut sbuf = vec![0u8; 1000];
    let mut rbuf = vec![0u8; 1500];

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, a1) = create_new_association_pair(&br, Arc::new(ca), Arc::new(cb)).await?;

    let cfg = Config {
        channel_type: ChannelType::ReliableUnordered,
        reliability_parameter: 123,
        label: "data".to_string(),
        ..Default::default()
    };

    let dc0 = DataChannel::dial(&a0, 100, cfg.clone()).await?;
    bridge_process_at_least_one(&br).await;

    let existing_data_channels: Vec<DataChannel> = Vec::new();
    let dc1 = DataChannel::accept(&a1, Config::default(), &existing_data_channels).await?;
    bridge_process_at_least_one(&br).await;

    assert_eq!(dc0.config, cfg, "local config should match");
    assert_eq!(dc1.config, cfg, "remote config should match");

    dc0.commit_reliability_params();
    dc1.commit_reliability_params();

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = dc0
        .write_data_channel(&Bytes::from(sbuf.clone()), true)
        .await?;
    assert_eq!(sbuf.len(), n, "data length should match");

    sbuf[0..4].copy_from_slice(&2u32.to_be_bytes());
    let n = dc0
        .write_data_channel(&Bytes::from(sbuf.clone()), true)
        .await?;
    assert_eq!(sbuf.len(), n, "data length should match");

    tokio::time::sleep(Duration::from_millis(100)).await;
    br.reorder(0).await; // reordering on the wire
    bridge_process_at_least_one(&br).await;

    let (n, is_string) = dc1.read_data_channel(&mut rbuf[..]).await?;
    assert!(is_string, "should return isString being true");
    assert_eq!(sbuf.len(), n, "data length should match");
    assert_eq!(
        2,
        u32::from_be_bytes([rbuf[0], rbuf[1], rbuf[2], rbuf[3]]),
        "data should match"
    );

    let (n, is_string) = dc1.read_data_channel(&mut rbuf[..]).await?;
    assert!(is_string, "should return isString being true");
    assert_eq!(sbuf.len(), n, "data length should match");
    assert_eq!(
        1,
        u32::from_be_bytes([rbuf[0], rbuf[1], rbuf[2], rbuf[3]]),
        "data should match"
    );

    dc0.close().await?;
    dc1.close().await?;
    bridge_process_at_least_one(&br).await;

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

#[cfg(not(target_os = "windows"))] // this times out in CI on windows.
#[tokio::test]
async fn test_data_channel_channel_type_partial_reliable_rexmit() -> Result<()> {
    pr_ordered_unordered_test(ChannelType::PartialReliableRexmit, true).await
}

#[cfg(not(target_os = "windows"))] // this times out in CI on windows.
#[tokio::test]
async fn test_data_channel_channel_type_partial_reliable_rexmit_unordered() -> Result<()> {
    pr_ordered_unordered_test(ChannelType::PartialReliableRexmitUnordered, false).await
}

#[cfg(not(target_os = "windows"))] // this times out in CI on windows.
#[tokio::test]
async fn test_data_channel_channel_type_partial_reliable_timed() -> Result<()> {
    pr_ordered_unordered_test(ChannelType::PartialReliableTimed, true).await
}

#[cfg(not(target_os = "windows"))] // this times out in CI on windows.
#[tokio::test]
async fn test_data_channel_channel_type_partial_reliable_timed_unordered() -> Result<()> {
    pr_ordered_unordered_test(ChannelType::PartialReliableTimedUnordered, false).await
}

//TODO: remove this conditional test
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[tokio::test]
async fn test_data_channel_buffered_amount() -> Result<()> {
    let sbuf = vec![0u8; 1000];
    let mut rbuf = vec![0u8; 1000];

    let n_cbs = Arc::new(AtomicUsize::new(0));

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, a1) = create_new_association_pair(&br, Arc::new(ca), Arc::new(cb)).await?;

    let dc0 = Arc::new(
        DataChannel::dial(
            &a0,
            100,
            Config {
                label: "data".to_owned(),
                ..Default::default()
            },
        )
        .await?,
    );
    bridge_process_at_least_one(&br).await;

    let existing_data_channels: Vec<DataChannel> = Vec::new();
    let dc1 = Arc::new(DataChannel::accept(&a1, Config::default(), &existing_data_channels).await?);
    bridge_process_at_least_one(&br).await;

    while dc0.buffered_amount() > 0 {
        bridge_process_at_least_one(&br).await;
    }

    let n = dc0.write(&Bytes::new()).await?;
    assert_eq!(n, 0, "data length should match");
    assert_eq!(dc0.buffered_amount(), 1, "incorrect bufferedAmount");

    let n = dc0.write(&Bytes::from_static(&[0])).await?;
    assert_eq!(n, 1, "data length should match");
    assert_eq!(dc0.buffered_amount(), 2, "incorrect bufferedAmount");

    bridge_process_at_least_one(&br).await;

    let n = dc1.read(&mut rbuf[..]).await?;
    assert_eq!(n, 0, "received length should match");

    let n = dc1.read(&mut rbuf[..]).await?;
    assert_eq!(n, 1, "received length should match");

    dc0.set_buffered_amount_low_threshold(1500);
    assert_eq!(
        dc0.buffered_amount_low_threshold(),
        1500,
        "incorrect bufferedAmountLowThreshold"
    );
    let n_cbs2 = Arc::clone(&n_cbs);
    dc0.on_buffered_amount_low(Box::new(move || {
        n_cbs2.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }));

    // Write 10 1000-byte packets (total 10,000 bytes)
    for i in 0..10 {
        let n = dc0.write(&Bytes::from(sbuf.clone())).await?;
        assert_eq!(sbuf.len(), n, "data length should match");
        assert_eq!(
            sbuf.len() * (i + 1) + 2,
            dc0.buffered_amount(),
            "incorrect bufferedAmount"
        );
    }

    let dc1_cloned = Arc::clone(&dc1);
    tokio::spawn(async move {
        while let Ok(n) = dc1_cloned.read(&mut rbuf[..]).await {
            if n == 0 {
                break;
            }
            assert_eq!(n, rbuf.len(), "received length should match");
        }
    });

    let since = tokio::time::Instant::now();
    loop {
        br.tick().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        if tokio::time::Instant::now().duration_since(since) > Duration::from_millis(500) {
            break;
        }
    }

    dc0.close().await?;
    dc1.close().await?;
    bridge_process_at_least_one(&br).await;

    assert!(
        n_cbs.load(Ordering::SeqCst) > 0,
        "should make at least one callback"
    );

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//TODO: remove this conditional test
#[cfg(not(any(target_os = "macos", target_os = "windows")))] // this times out in CI on windows.
#[tokio::test]
async fn test_stats() -> Result<()> {
    let sbuf = vec![0u8; 1000];
    let mut rbuf = vec![0u8; 1500];

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, a1) = create_new_association_pair(&br, Arc::new(ca), Arc::new(cb)).await?;

    let cfg = Config {
        channel_type: ChannelType::Reliable,
        reliability_parameter: 123,
        label: "data".to_owned(),
        ..Default::default()
    };

    let dc0 = DataChannel::dial(&a0, 100, cfg.clone()).await?;
    bridge_process_at_least_one(&br).await;

    let existing_data_channels: Vec<DataChannel> = Vec::new();
    let dc1 = DataChannel::accept(&a1, Config::default(), &existing_data_channels).await?;
    bridge_process_at_least_one(&br).await;

    let mut bytes_sent = 0;

    let n = dc0.write(&Bytes::from(sbuf.clone())).await?;
    assert_eq!(n, sbuf.len(), "data length should match");
    bytes_sent += n;

    assert_eq!(dc0.bytes_sent(), bytes_sent);
    assert_eq!(dc0.messages_sent(), 1);

    let n = dc0.write(&Bytes::from(sbuf.clone())).await?;
    assert_eq!(n, sbuf.len(), "data length should match");
    bytes_sent += n;

    assert_eq!(dc0.bytes_sent(), bytes_sent);
    assert_eq!(dc0.messages_sent(), 2);

    let n = dc0.write(&Bytes::from_static(&[0])).await?;
    assert_eq!(n, 1, "data length should match");
    bytes_sent += n;

    assert_eq!(dc0.bytes_sent(), bytes_sent);
    assert_eq!(dc0.messages_sent(), 3);

    let n = dc0.write(&Bytes::from_static(&[])).await?;
    assert_eq!(n, 0, "data length should match");
    bytes_sent += n;

    assert_eq!(dc0.bytes_sent(), bytes_sent);
    assert_eq!(dc0.messages_sent(), 4);

    bridge_process_at_least_one(&br).await;

    let mut bytes_read = 0;

    let n = dc1.read(&mut rbuf[..]).await?;
    assert_eq!(n, sbuf.len(), "data length should match");
    bytes_read += n;

    assert_eq!(dc1.bytes_received(), bytes_read);
    assert_eq!(dc1.messages_received(), 1);

    let n = dc1.read(&mut rbuf[..]).await?;
    assert_eq!(n, sbuf.len(), "data length should match");
    bytes_read += n;

    assert_eq!(dc1.bytes_received(), bytes_read);
    assert_eq!(dc1.messages_received(), 2);

    let n = dc1.read(&mut rbuf[..]).await?;
    assert_eq!(n, 1, "data length should match");
    bytes_read += n;

    assert_eq!(dc1.bytes_received(), bytes_read);
    assert_eq!(dc1.messages_received(), 3);

    let n = dc1.read(&mut rbuf[..]).await?;
    assert_eq!(n, 0, "data length should match");
    bytes_read += n;

    assert_eq!(dc1.bytes_received(), bytes_read);
    assert_eq!(dc1.messages_received(), 4);

    dc0.close().await?;
    dc1.close().await?;
    bridge_process_at_least_one(&br).await;

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

#[tokio::test]
async fn test_poll_data_channel() -> Result<()> {
    let mut sbuf = vec![0u8; 1000];
    let mut rbuf = vec![0u8; 1500];

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, a1) = create_new_association_pair(&br, Arc::new(ca), Arc::new(cb)).await?;

    let cfg = Config {
        channel_type: ChannelType::Reliable,
        reliability_parameter: 123,
        label: "data".to_string(),
        ..Default::default()
    };

    let dc0 = Arc::new(DataChannel::dial(&a0, 100, cfg.clone()).await?);
    bridge_process_at_least_one(&br).await;

    let existing_data_channels: Vec<DataChannel> = Vec::new();
    let dc1 = Arc::new(DataChannel::accept(&a1, Config::default(), &existing_data_channels).await?);
    bridge_process_at_least_one(&br).await;

    let mut poll_dc0 = PollDataChannel::new(dc0);
    let mut poll_dc1 = PollDataChannel::new(dc1);

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = poll_dc0
        .write(&Bytes::from(sbuf.clone()))
        .await
        .map_err(|e| Error::new(e.to_string()))?;
    assert_eq!(sbuf.len(), n, "data length should match");

    bridge_process_at_least_one(&br).await;

    let n = poll_dc1
        .read(&mut rbuf[..])
        .await
        .map_err(|e| Error::new(e.to_string()))?;
    assert_eq!(sbuf.len(), n, "data length should match");
    assert_eq!(
        1,
        u32::from_be_bytes([rbuf[0], rbuf[1], rbuf[2], rbuf[3]]),
        "data should match"
    );

    poll_dc0.into_inner().close().await?;
    poll_dc1.into_inner().close().await?;
    bridge_process_at_least_one(&br).await;

    close_association_pair(&br, a0, a1).await;

    Ok(())
}
