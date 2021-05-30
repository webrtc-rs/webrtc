use super::*;
use crate::stream::*;

use std::time::Duration;
use util::conn::conn_bridge::*;

async fn create_new_association_pair(
    br: &Arc<Bridge>,
    ca: Arc<dyn Conn + Send + Sync>,
    cb: Arc<dyn Conn + Send + Sync>,
    ack_mode: AckMode,
    recv_buf_size: u32,
) -> Result<(Association, Association), Error> {
    let (handshake0ch_tx, mut handshake0ch_rx) = mpsc::channel(1);
    let (handshake1ch_tx, mut handshake1ch_rx) = mpsc::channel(1);
    let (closed_tx, mut closed_rx0) = broadcast::channel::<()>(1);
    let mut closed_rx1 = closed_tx.subscribe();

    // Setup client
    tokio::spawn(async move {
        let client = Association::client(Config {
            net_conn: ca,
            max_receive_buffer_size: recv_buf_size,
            max_message_size: 0,
            name: "client".to_owned(),
        })
        .await;

        let _ = handshake0ch_tx.send(client).await;
        let _ = closed_rx0.recv().await;

        Ok::<(), Error>(())
    });

    // Setup server
    tokio::spawn(async move {
        let server = Association::server(Config {
            net_conn: cb,
            max_receive_buffer_size: recv_buf_size,
            max_message_size: 0,
            name: "server".to_owned(),
        })
        .await;

        let _ = handshake1ch_tx.send(server).await;
        let _ = closed_rx1.recv().await;

        Ok::<(), Error>(())
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
        return Err(Error::ErrOthers("handshake failed".to_owned()));
    }

    drop(closed_tx);

    let (client, server) = (client.unwrap(), server.unwrap());
    {
        let mut ai = client.association_internal.lock().await;
        ai.ack_mode = ack_mode;
    }
    {
        let mut ai = server.association_internal.lock().await;
        ai.ack_mode = ack_mode;
    }

    Ok((client, server))
}

async fn close_association_pair(br: &Arc<Bridge>, client: Association, server: Association) {
    let (handshake0ch_tx, mut handshake0ch_rx) = mpsc::channel(1);
    let (handshake1ch_tx, mut handshake1ch_rx) = mpsc::channel(1);
    let (closed_tx, mut closed_rx0) = broadcast::channel::<()>(1);
    let mut closed_rx1 = closed_tx.subscribe();

    // Close client
    tokio::spawn(async move {
        client.close().await?;
        let _ = handshake0ch_tx.send(()).await;
        let _ = closed_rx0.recv().await;

        Ok::<(), Error>(())
    });

    // Close server
    tokio::spawn(async move {
        server.close().await?;
        let _ = handshake1ch_tx.send(()).await;
        let _ = closed_rx1.recv().await;

        Ok::<(), Error>(())
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

async fn flush_buffers(br: &Arc<Bridge>, client: &Association, server: &Association) {
    loop {
        loop {
            let n = br.tick().await;
            if n == 0 {
                break;
            }
        }

        {
            let (a0, a1) = (
                client.association_internal.lock().await,
                server.association_internal.lock().await,
            );
            if a0.buffered_amount() == 0 && a1.buffered_amount() == 0 {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn establish_session_pair(
    br: &Arc<Bridge>,
    client: &Association,
    server: &mut Association,
    si: u16,
) -> Result<(Arc<Stream>, Arc<Stream>), Error> {
    let hello_msg = Bytes::from_static(b"Hello");
    let s0 = client
        .open_stream(si, PayloadProtocolIdentifier::Binary)
        .await?;
    let _ = s0
        .write_sctp(&hello_msg, PayloadProtocolIdentifier::Dcep)
        .await?;

    flush_buffers(br, client, server).await;

    let s1 = server.accept_stream().await.unwrap();
    if s0.stream_identifier != s1.stream_identifier {
        return Err(Error::ErrOthers("SI should match".to_owned()));
    }

    br.process().await;

    let mut buf = vec![0u8; 1024];
    let (n, ppi) = s1.read_sctp(&mut buf).await?;

    if n != hello_msg.len() {
        return Err(Error::ErrOthers("received data must by 3 bytes".to_owned()));
    }

    if ppi != PayloadProtocolIdentifier::Dcep {
        return Err(Error::ErrOthers("unexpected ppi".to_owned()));
    }

    if &buf[..n] != &hello_msg {
        return Err(Error::ErrOthers("received data mismatch".to_owned()));
    }

    flush_buffers(br, client, server).await;

    Ok((s0, s1))
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_reliable_simple() -> Result<(), Error> {
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

    const SI: u16 = 1;
    const MSG: Bytes = Bytes::from_static(b"ABC");

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    {
        let a = a0.association_internal.lock().await;
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    let n = s0
        .write_sctp(&MSG, PayloadProtocolIdentifier::Binary)
        .await?;
    assert_eq!(MSG.len(), n, "unexpected length of received data");
    {
        let a = a0.association_internal.lock().await;
        assert_eq!(MSG.len(), a.buffered_amount(), "incorrect bufferedAmount");
    }

    flush_buffers(&br, &a0, &a1).await;

    let mut buf = vec![0u8; 32];
    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, MSG.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    {
        let a = a0.association_internal.lock().await;
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_reliable_ordered_reordered() -> Result<(), Error> {
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

    const SI: u16 = 2;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }
    let mut sbufl = vec![0u8; 2000];
    for i in 0..sbufl.len() {
        sbufl[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    {
        let a = a0.association_internal.lock().await;
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    tokio::time::sleep(Duration::from_millis(10)).await;
    br.reorder(0).await;
    br.process().await;

    let mut buf = vec![0u8; 2000];

    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        0,
        "unexpected received data"
    );

    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    br.process().await;

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_reliable_ordered_fragmented_then_defragmented() -> Result<(), Error> {
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

    const SI: u16 = 3;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }
    let mut sbufl = vec![0u8; 2000];
    for i in 0..sbufl.len() {
        sbufl[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    s0.set_reliability_params(false, ReliabilityType::Reliable, 0);
    s1.set_reliability_params(false, ReliabilityType::Reliable, 0);

    let n = s0
        .write_sctp(
            &Bytes::from(sbufl.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbufl.len(), n, "unexpected length of received data");

    flush_buffers(&br, &a0, &a1).await;

    let mut rbuf = vec![0u8; 2000];
    let (n, ppi) = s1.read_sctp(&mut rbuf).await?;
    assert_eq!(n, sbufl.len(), "unexpected length of received data");
    assert_eq!(&rbuf[..n], &sbufl, "unexpected received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

    br.process().await;

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_reliable_unordered_fragmented_then_defragmented() -> Result<(), Error> {
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

    const SI: u16 = 4;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }
    let mut sbufl = vec![0u8; 2000];
    for i in 0..sbufl.len() {
        sbufl[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    s0.set_reliability_params(true, ReliabilityType::Reliable, 0);
    s1.set_reliability_params(true, ReliabilityType::Reliable, 0);

    let n = s0
        .write_sctp(
            &Bytes::from(sbufl.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbufl.len(), n, "unexpected length of received data");

    flush_buffers(&br, &a0, &a1).await;

    let mut rbuf = vec![0u8; 2000];
    let (n, ppi) = s1.read_sctp(&mut rbuf).await?;
    assert_eq!(n, sbufl.len(), "unexpected length of received data");
    assert_eq!(&rbuf[..n], &sbufl, "unexpected received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

    br.process().await;

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_reliable_unordered_ordered() -> Result<(), Error> {
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

    const SI: u16 = 5;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }
    let mut sbufl = vec![0u8; 2000];
    for i in 0..sbufl.len() {
        sbufl[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    s0.set_reliability_params(true, ReliabilityType::Reliable, 0);
    s1.set_reliability_params(true, ReliabilityType::Reliable, 0);

    br.reorder_next_nwrites(0, 2).await;

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    flush_buffers(&br, &a0, &a1).await;

    let mut buf = vec![0u8; 2000];

    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        0,
        "unexpected received data"
    );

    br.process().await;

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_reliable_retransmission() -> Result<(), Error> {
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

    const SI: u16 = 6;
    const MSG1: Bytes = Bytes::from_static(b"ABC");
    const MSG2: Bytes = Bytes::from_static(b"DEFG");

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;
    {
        let mut a = a0.association_internal.lock().await;
        a.rto_mgr.set_rto(100, true);
    }

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    let n = s0
        .write_sctp(&MSG1, PayloadProtocolIdentifier::Binary)
        .await?;
    assert_eq!(MSG1.len(), n, "unexpected length of received data");

    let n = s0
        .write_sctp(&MSG2, PayloadProtocolIdentifier::Binary)
        .await?;
    assert_eq!(MSG2.len(), n, "unexpected length of received data");

    tokio::time::sleep(Duration::from_millis(10)).await;
    log::debug!("dropping packet");
    br.drop_offset(0, 0, 1).await; // drop the first packet (second one should be sacked)

    // process packets for 200 msec
    for _ in 0..20 {
        br.tick().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let mut buf = vec![0u8; 32];

    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, MSG1.len(), "unexpected length of received data");
    assert_eq!(&buf[..n], &MSG1, "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, MSG2.len(), "unexpected length of received data");
    assert_eq!(&buf[..n], &MSG2, "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

    br.process().await;

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_reliable_short_buffer() -> Result<(), Error> {
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

    const SI: u16 = 1;
    const MSG: Bytes = Bytes::from_static(b"Hello");

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    {
        let a = a0.association_internal.lock().await;
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    let n = s0
        .write_sctp(&MSG, PayloadProtocolIdentifier::Binary)
        .await?;
    assert_eq!(MSG.len(), n, "unexpected length of received data");
    {
        let a = a0.association_internal.lock().await;
        assert_eq!(MSG.len(), a.buffered_amount(), "incorrect bufferedAmount");
    }

    flush_buffers(&br, &a0, &a1).await;

    let mut buf = vec![0u8; 3];
    let result = s1.read_sctp(&mut buf).await;
    assert!(result.is_err(), "expected error to be io.ErrShortBuffer");
    if let Err(err) = result {
        assert_eq!(
            err,
            Error::ErrShortBuffer,
            "expected error to be io.ErrShortBuffer"
        );
    }

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    {
        let a = a0.association_internal.lock().await;
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_unreliable_rexmit_ordered_no_fragment() -> Result<(), Error> {
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

    const SI: u16 = 1;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    s0.set_reliability_params(false, ReliabilityType::Rexmit, 0);
    s1.set_reliability_params(false, ReliabilityType::Rexmit, 0); // doesn't matter

    br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    log::debug!("flush_buffers");
    flush_buffers(&br, &a0, &a1).await;

    let mut buf = vec![0u8; 2000];

    log::debug!("read_sctp");
    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    log::debug!("process");
    br.process().await;

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_unreliable_rexmit_ordered_fragment() -> Result<(), Error> {
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

    const SI: u16 = 1;
    let mut sbuf = vec![0u8; 2000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    {
        // lock RTO value at 100 [msec]
        let mut a = a0.association_internal.lock().await;
        a.rto_mgr.set_rto(100, true);
    }
    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    s0.set_reliability_params(false, ReliabilityType::Rexmit, 0);
    s1.set_reliability_params(false, ReliabilityType::Rexmit, 0); // doesn't matter

    br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    //log::debug!("flush_buffers");
    flush_buffers(&br, &a0, &a1).await;

    let mut buf = vec![0u8; 2000];

    //log::debug!("read_sctp");
    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    //log::debug!("process");
    br.process().await;

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_unreliable_rexmit_unordered_no_fragment() -> Result<(), Error> {
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

    const SI: u16 = 2;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    s0.set_reliability_params(true, ReliabilityType::Rexmit, 0);
    s1.set_reliability_params(true, ReliabilityType::Rexmit, 0); // doesn't matter

    br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    //log::debug!("flush_buffers");
    flush_buffers(&br, &a0, &a1).await;

    let mut buf = vec![0u8; 2000];

    //log::debug!("read_sctp");
    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    //log::debug!("process");
    br.process().await;

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_unreliable_rexmit_unordered_fragment() -> Result<(), Error> {
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

    const SI: u16 = 1;
    let mut sbuf = vec![0u8; 2000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    s0.set_reliability_params(true, ReliabilityType::Rexmit, 0);
    s1.set_reliability_params(true, ReliabilityType::Rexmit, 0); // doesn't matter

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    //log::debug!("flush_buffers");
    tokio::time::sleep(Duration::from_millis(10)).await;
    br.drop_offset(0, 0, 2).await; // drop the second fragment of the first chunk (second chunk should be sacked)
    flush_buffers(&br, &a0, &a1).await;

    let mut buf = vec![0u8; 2000];

    //log::debug!("read_sctp");
    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    //log::debug!("process");
    br.process().await;

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
        assert_eq!(
            0,
            q.unordered.len(),
            "should be nothing in the unordered queue"
        );
        assert_eq!(
            0,
            q.unordered_chunks.len(),
            "should be nothing in the unorderedChunks list"
        );
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_unreliable_rexmit_timed_ordered() -> Result<(), Error> {
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

    const SI: u16 = 3;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    s0.set_reliability_params(false, ReliabilityType::Timed, 0);
    s1.set_reliability_params(false, ReliabilityType::Timed, 0); // doesn't matter

    br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    //log::debug!("flush_buffers");
    flush_buffers(&br, &a0, &a1).await;

    let mut buf = vec![0u8; 2000];

    //log::debug!("read_sctp");
    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    //log::debug!("process");
    br.process().await;

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_unreliable_rexmit_timed_unordered() -> Result<(), Error> {
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

    const SI: u16 = 3;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    // When we set the reliability value to 0 [times], then it will cause
    // the chunk to be abandoned immediately after the first transmission.
    s0.set_reliability_params(true, ReliabilityType::Timed, 0);
    s1.set_reliability_params(true, ReliabilityType::Timed, 0); // doesn't matter

    br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    //log::debug!("flush_buffers");
    flush_buffers(&br, &a0, &a1).await;

    let mut buf = vec![0u8; 2000];

    //log::debug!("read_sctp");
    let (n, ppi) = s1.read_sctp(&mut buf).await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");
    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
    assert_eq!(
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        1,
        "unexpected received data"
    );

    //log::debug!("process");
    br.process().await;

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
        assert_eq!(
            0,
            q.unordered.len(),
            "should be nothing in the unordered queue"
        );
        assert_eq!(
            0,
            q.unordered_chunks.len(),
            "should be nothing in the unorderedChunks list"
        );
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//TODO: TestAssocT1InitTimer
//TODO: TestAssocT1CookieTimer
//TODO: TestAssocT3RtxTimer

//use std::io::Write;

// 1) Send 4 packets. drop the first one.
// 2) Last 3 packet will be received, which triggers fast-retransmission
// 3) The first one is retransmitted, which makes s1 readable
// Above should be done before RTO occurs (fast recovery)
#[tokio::test]
async fn test_assoc_congestion_control_fast_retransmission() -> Result<(), Error> {
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

    const SI: u16 = 6;
    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::Normal, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    br.drop_next_nwrites(0, 1).await; // drop the first packet (second one should be sacked)

    for i in 0..4u32 {
        sbuf[0..4].copy_from_slice(&i.to_be_bytes());
        let n = s0
            .write_sctp(
                &Bytes::from(sbuf.clone()),
                PayloadProtocolIdentifier::Binary,
            )
            .await?;
        assert_eq!(sbuf.len(), n, "unexpected length of received data");
    }

    // process packets for 500 msec, assuming that the fast retrans/recover
    // should complete within 500 msec.
    for _ in 0..50 {
        br.tick().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let mut buf = vec![0u8; 3000];

    // Try to read all 4 packets
    for i in 0..4 {
        {
            let q = s1.reassembly_queue.lock().await;
            assert!(q.is_readable(), "should be readable");
        }

        let (n, ppi) = s1.read_sctp(&mut buf).await?;
        assert_eq!(n, sbuf.len(), "unexpected length of received data");
        assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
        assert_eq!(
            u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
            i,
            "unexpected received data"
        );
    }

    //br.process().await;

    {
        let a = a0.association_internal.lock().await;
        let b = a1.association_internal.lock().await;
        assert!(!a.in_fast_recovery, "should not be in fast-recovery");

        log::debug!("nDATAs      : {}", b.stats.get_num_datas());
        log::debug!("nSACKs      : {}", a.stats.get_num_sacks());
        log::debug!("nAckTimeouts: {}", b.stats.get_num_ack_timeouts());
        log::debug!("nFastRetrans: {}", a.stats.get_num_fast_retrans());

        assert_eq!(1, a.stats.get_num_fast_retrans(), "should be 1");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_congestion_control_congestion_avoidance() -> Result<(), Error> {
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

    const MAX_RECEIVE_BUFFER_SIZE: u32 = 64 * 1024;
    const SI: u16 = 6;
    const N_PACKETS_TO_SEND: u32 = 2000;

    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) = create_new_association_pair(
        &br,
        Arc::new(ca),
        Arc::new(cb),
        AckMode::Normal,
        MAX_RECEIVE_BUFFER_SIZE,
    )
    .await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    {
        let a = a0.association_internal.lock().await;
        let b = a1.association_internal.lock().await;
        a.stats.reset();
        b.stats.reset();
    }

    for i in 0..N_PACKETS_TO_SEND {
        sbuf[0..4].copy_from_slice(&i.to_be_bytes());
        let n = s0
            .write_sctp(
                &Bytes::from(sbuf.clone()),
                PayloadProtocolIdentifier::Binary,
            )
            .await?;
        assert_eq!(sbuf.len(), n, "unexpected length of received data");
    }

    let mut rbuf = vec![0u8; 3000];

    // Repeat calling br.Tick() until the buffered amount becomes 0
    let mut n_packets_received = 0u32;
    while s0.buffered_amount() > 0 && n_packets_received < N_PACKETS_TO_SEND {
        loop {
            let n = br.tick().await;
            if n == 0 {
                break;
            }
        }

        loop {
            let readable = {
                let q = s1.reassembly_queue.lock().await;
                q.is_readable()
            };
            if !readable {
                break;
            }
            let (n, ppi) = s1.read_sctp(&mut rbuf).await?;
            assert_eq!(sbuf.len(), n, "unexpected length of received data");
            assert_eq!(
                n_packets_received,
                u32::from_be_bytes([rbuf[0], rbuf[1], rbuf[2], rbuf[3]]),
                "unexpected length of received data"
            );
            assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

            n_packets_received += 1;
        }
    }

    br.process().await;

    assert_eq!(
        n_packets_received, N_PACKETS_TO_SEND,
        "unexpected num of packets received"
    );

    {
        let a = a0.association_internal.lock().await;
        let b = a1.association_internal.lock().await;

        assert!(!a.in_fast_recovery, "should not be in fast-recovery");
        assert!(
            a.cwnd > a.ssthresh,
            "should be in congestion avoidance mode"
        );
        assert!(
            a.ssthresh >= MAX_RECEIVE_BUFFER_SIZE,
            "{} should not be less than the initial size of 128KB {}",
            a.ssthresh,
            MAX_RECEIVE_BUFFER_SIZE
        );

        assert_eq!(
            0,
            s1.get_num_bytes_in_reassembly_queue().await,
            "reassembly queue should be empty"
        );

        log::debug!("nDATAs      : {}", b.stats.get_num_datas());
        log::debug!("nSACKs      : {}", a.stats.get_num_sacks());
        log::debug!("nT3Timeouts: {}", a.stats.get_num_t3timeouts());

        assert_eq!(
            N_PACKETS_TO_SEND as u64,
            b.stats.get_num_datas(),
            "packet count mismatch"
        );
        assert!(
            a.stats.get_num_sacks() <= N_PACKETS_TO_SEND as u64 / 2,
            "too many sacks"
        );
        assert_eq!(0, a.stats.get_num_t3timeouts(), "should be no retransmit");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_congestion_control_slow_reader() -> Result<(), Error> {
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

    const MAX_RECEIVE_BUFFER_SIZE: u32 = 64 * 1024;
    const SI: u16 = 6;
    const N_PACKETS_TO_SEND: u32 = 130;

    let mut sbuf = vec![0u8; 1000];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) = create_new_association_pair(
        &br,
        Arc::new(ca),
        Arc::new(cb),
        AckMode::Normal,
        MAX_RECEIVE_BUFFER_SIZE,
    )
    .await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    for i in 0..N_PACKETS_TO_SEND {
        sbuf[0..4].copy_from_slice(&i.to_be_bytes());
        let n = s0
            .write_sctp(
                &Bytes::from(sbuf.clone()),
                PayloadProtocolIdentifier::Binary,
            )
            .await?;
        assert_eq!(sbuf.len(), n, "unexpected length of received data");
    }

    let mut rbuf = vec![0u8; 3000];

    // 1. First forward packets to receiver until rwnd becomes 0
    // 2. Wait until the sender's cwnd becomes 1*MTU (RTO occurred)
    // 3. Stat reading a1's data
    let mut n_packets_received = 0u32;
    let mut has_rtoed = false;
    while s0.buffered_amount() > 0 && n_packets_received < N_PACKETS_TO_SEND {
        loop {
            let n = br.tick().await;
            if n == 0 {
                break;
            }
        }

        if !has_rtoed {
            let a = a0.association_internal.lock().await;
            let b = a1.association_internal.lock().await;

            let rwnd = b.get_my_receiver_window_credit().await;
            let cwnd = a.cwnd;
            if cwnd > a.mtu || rwnd > 0 {
                // Do not read until a1.getMyReceiverWindowCredit() becomes zero
                continue;
            }

            has_rtoed = true;
        }

        loop {
            let readable = {
                let q = s1.reassembly_queue.lock().await;
                q.is_readable()
            };
            if !readable {
                break;
            }
            let (n, ppi) = s1.read_sctp(&mut rbuf).await?;
            assert_eq!(sbuf.len(), n, "unexpected length of received data");
            assert_eq!(
                n_packets_received,
                u32::from_be_bytes([rbuf[0], rbuf[1], rbuf[2], rbuf[3]]),
                "unexpected length of received data"
            );
            assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

            n_packets_received += 1;
        }

        tokio::time::sleep(Duration::from_millis(4)).await;
    }

    br.process().await;

    assert_eq!(
        n_packets_received, N_PACKETS_TO_SEND,
        "unexpected num of packets received"
    );
    assert_eq!(
        0,
        s1.get_num_bytes_in_reassembly_queue().await,
        "reassembly queue should be empty"
    );

    {
        let a = a0.association_internal.lock().await;
        let b = a1.association_internal.lock().await;

        log::debug!("nDATAs      : {}", b.stats.get_num_datas());
        log::debug!("nSACKs      : {}", a.stats.get_num_sacks());
        log::debug!("nAckTimeouts: {}", b.stats.get_num_ack_timeouts());
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

/*FIXME
use std::io::Write;

#[tokio::test]
async fn test_assoc_delayed_ack() -> Result<(), Error> {
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
        .filter(None, log::LevelFilter::Trace)
        .init();

    const SI: u16 = 6;
    let mut sbuf = vec![0u8; 1000];
    let mut rbuf = vec![0u8; 1500];
    for i in 0..sbuf.len() {
        sbuf[i] = (i & 0xff) as u8;
    }

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::AlwaysDelay, 0)
            .await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    {
        let a = a0.association_internal.lock().await;
        let b = a1.association_internal.lock().await;
        a.stats.reset();
        b.stats.reset();
    }

    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(sbuf.len(), n, "unexpected length of received data");

    // Repeat calling br.Tick() until the buffered amount becomes 0
    let since = SystemTime::now();
    let mut n_packets_received = 0;
    while s0.buffered_amount() > 0 {
        loop {
            let n = br.tick().await;
            if n == 0 {
                break;
            }
        }

        loop {
            let readable = {
                let q = s1.reassembly_queue.lock().await;
                q.is_readable()
            };
            if !readable {
                break;
            }
            let (n, ppi) = s1.read_sctp(&mut rbuf).await?;
            assert_eq!(sbuf.len(), n, "unexpected length of received data");
            assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");

            n_packets_received += 1;
        }
    }
    let delay = (SystemTime::now().duration_since(since).unwrap().as_millis() as f64) / 1000.0;
    log::debug!("received in {} seconds", delay);
    assert!(delay >= 0.2, "should be >= 200msec");

    br.process().await;

    assert_eq!(n_packets_received, 1, "unexpected num of packets received");
    assert_eq!(
        0,
        s1.get_num_bytes_in_reassembly_queue().await,
        "reassembly queue should be empty"
    );

    {
        let a = a0.association_internal.lock().await;
        let b = a1.association_internal.lock().await;

        log::debug!("nDATAs      : {}", b.stats.get_num_datas());
        log::debug!("nSACKs      : {}", a.stats.get_num_sacks());
        log::debug!("nAckTimeouts: {}", b.stats.get_num_ack_timeouts());

        assert_eq!(1, b.stats.get_num_datas(), "DATA chunk count mismatch");
        assert_eq!(
            a.stats.get_num_sacks(),
            b.stats.get_num_datas(),
            "sack count should be equal to the number of data chunks"
        );
        assert_eq!(
            1,
            b.stats.get_num_ack_timeouts(),
            "ackTimeout count mismatch"
        );
        assert_eq!(0, a.stats.get_num_t3timeouts(), "should be no retransmit");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}
*/

//use std::io::Write;

#[tokio::test]
async fn test_assoc_reset_close_one_way() -> Result<(), Error> {
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

    const SI: u16 = 1;
    const MSG: Bytes = Bytes::from_static(b"ABC");

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    {
        let a = a0.association_internal.lock().await;
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    let n = s0
        .write_sctp(&MSG, PayloadProtocolIdentifier::Binary)
        .await?;
    assert_eq!(MSG.len(), n, "unexpected length of received data");
    {
        let a = a0.association_internal.lock().await;
        assert_eq!(MSG.len(), a.buffered_amount(), "incorrect bufferedAmount");
    }

    log::debug!("s0.close");
    s0.close().await?; // send reset

    let (done_ch_tx, mut done_ch_rx) = mpsc::channel(1);
    let mut buf = vec![0u8; 32];

    tokio::spawn(async move {
        loop {
            log::debug!("s1.read_sctp begin");
            match s1.read_sctp(&mut buf).await {
                Ok((n, ppi)) => {
                    log::debug!("s1.read_sctp done with {:?}", &buf[..n]);
                    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
                    assert_eq!(n, MSG.len(), "unexpected length of received data");
                    let _ = done_ch_tx.send(None).await;
                }
                Err(err) => {
                    log::debug!("s1.read_sctp err {:?}", err);
                    let _ = done_ch_tx.send(Some(err)).await;
                    break;
                }
            }
        }
    });

    loop {
        br.process().await;

        let timer = tokio::time::sleep(Duration::from_millis(10));
        tokio::pin!(timer);

        tokio::select! {
            _ = timer.as_mut() =>{},
            result = done_ch_rx.recv() => {
                log::debug!("s1. {:?}", result);
                if let Some(err_opt) = result {
                    if let Some(err) = err_opt{
                        assert!(true, "got error {:?}", err);
                        break;
                    }
                }else{
                    break;
                }
            }
        }
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_reset_close_both_ways() -> Result<(), Error> {
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

    const SI: u16 = 1;
    const MSG: Bytes = Bytes::from_static(b"ABC");

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    {
        let a = a0.association_internal.lock().await;
        assert_eq!(0, a.buffered_amount(), "incorrect bufferedAmount");
    }

    let n = s0
        .write_sctp(&MSG, PayloadProtocolIdentifier::Binary)
        .await?;
    assert_eq!(MSG.len(), n, "unexpected length of received data");
    {
        let a = a0.association_internal.lock().await;
        assert_eq!(MSG.len(), a.buffered_amount(), "incorrect bufferedAmount");
    }

    log::debug!("s0.close");
    s0.close().await?; // send reset

    let (done_ch_tx, mut done_ch_rx) = mpsc::channel(1);
    let done_ch_tx = Arc::new(done_ch_tx);

    let done_ch_tx1 = Arc::clone(&done_ch_tx);
    let ss1 = Arc::clone(&s1);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 32];
        loop {
            log::debug!("s1.read_sctp begin");
            match ss1.read_sctp(&mut buf).await {
                Ok((n, ppi)) => {
                    log::debug!("s1.read_sctp done with {:?}", &buf[..n]);
                    assert_eq!(ppi, PayloadProtocolIdentifier::Binary, "unexpected ppi");
                    assert_eq!(n, MSG.len(), "unexpected length of received data");
                    let _ = done_ch_tx1.send(None).await;
                }
                Err(err) => {
                    log::debug!("s1.read_sctp err {:?}", err);
                    let _ = done_ch_tx1.send(Some(err)).await;
                    break;
                }
            }
        }
    });

    loop {
        br.process().await;

        let timer = tokio::time::sleep(Duration::from_millis(10));
        tokio::pin!(timer);

        tokio::select! {
            _ = timer.as_mut() =>{},
            result = done_ch_rx.recv() => {
                log::debug!("s1. {:?}", result);
                if let Some(err_opt) = result {
                    if let Some(err) = err_opt{
                        assert!(true, "got error {:?}", err);
                        break;
                    }
                }else{
                    break;
                }
            }
        }
    }

    log::debug!("s1.close");
    s1.close().await?; // send reset

    let done_ch_tx0 = Arc::clone(&done_ch_tx);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 32];
        loop {
            log::debug!("s.read_sctp begin");
            match s0.read_sctp(&mut buf).await {
                Ok(_) => {
                    assert!(false, "must be error");
                }
                Err(err) => {
                    log::debug!("s0.read_sctp err {:?}", err);
                    let _ = done_ch_tx0.send(Some(err)).await;
                    break;
                }
            }
        }
    });

    loop {
        br.process().await;

        let timer = tokio::time::sleep(Duration::from_millis(10));
        tokio::pin!(timer);

        tokio::select! {
            _ = timer.as_mut() =>{},
            result = done_ch_rx.recv() => {
                log::debug!("s0. {:?}", result);
                if let Some(err_opt) = result {
                    if let Some(err) = err_opt{
                        assert!(true, "got error {:?}", err);
                        break;
                    }else{
                        assert!(false, "must be error");
                    }
                }else{
                    break;
                }
            }
        }
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_abort() -> Result<(), Error> {
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

    const SI: u16 = 1;
    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let abort = ChunkAbort {
        error_causes: vec![ErrorCauseProtocolViolation {
            code: PROTOCOL_VIOLATION,
            ..Default::default()
        }],
    };

    let packet = {
        let a = a0.association_internal.lock().await;
        a.create_packet(vec![Box::new(abort)]).marshal()?
    };

    let (_s0, _s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    // Both associations are established
    assert_eq!(AssociationState::Established, a0.get_state());
    assert_eq!(AssociationState::Established, a1.get_state());

    let result = a0.net_conn.send(&packet).await;
    assert!(result.is_ok(), "must be ok");

    flush_buffers(&br, &a0, &a1).await;

    // There is a little delay before changing the state to closed
    tokio::time::sleep(Duration::from_millis(10)).await;

    // The receiving association should be closed because it got an ABORT
    assert_eq!(AssociationState::Established, a0.get_state());
    assert_eq!(AssociationState::Closed, a1.get_state());

    close_association_pair(&br, a0, a1).await;

    Ok(())
}
