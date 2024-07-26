// Silence warning on `for i in 0..vec.len() { … }`:
#![allow(clippy::needless_range_loop)]

use std::io;
use std::net::{Shutdown, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use tokio::net::UdpSocket;
use util::conn::conn_bridge::*;
use util::conn::conn_pipe::pipe;
use util::conn::*;

use super::*;
use crate::chunk::chunk_selective_ack::GapAckBlock;
use crate::stream::*;

async fn create_new_association_pair(
    br: &Arc<Bridge>,
    ca: Arc<dyn Conn + Send + Sync>,
    cb: Arc<dyn Conn + Send + Sync>,
    ack_mode: AckMode,
    recv_buf_size: u32,
) -> Result<(Association, Association)> {
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

        Result::<()>::Ok(())
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
        return Err(Error::Other("handshake failed".to_owned()));
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
) -> Result<(Arc<Stream>, Arc<Stream>)> {
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
        return Err(Error::Other("SI should match".to_owned()));
    }

    br.process().await;

    let mut buf = vec![0u8; 1024];
    let (n, ppi) = s1.read_sctp(&mut buf).await?;

    if n != hello_msg.len() {
        return Err(Error::Other("received data must by 3 bytes".to_owned()));
    }

    if ppi != PayloadProtocolIdentifier::Dcep {
        return Err(Error::Other("unexpected ppi".to_owned()));
    }

    if buf[..n] != hello_msg {
        return Err(Error::Other("received data mismatch".to_owned()));
    }

    flush_buffers(br, client, server).await;

    Ok((s0, s1))
}

//use std::io::Write;

#[cfg(not(target_os = "windows"))] // this times out in CI on windows.
#[tokio::test]
async fn test_assoc_reliable_simple() -> Result<()> {
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
    static MSG: Bytes = Bytes::from_static(b"ABC");

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    {
        let a = a0.association_internal.lock().await;
        assert_eq!(a.buffered_amount(), 0, "incorrect bufferedAmount");
    }

    let n = s0
        .write_sctp(&MSG, PayloadProtocolIdentifier::Binary)
        .await?;
    assert_eq!(n, MSG.len(), "unexpected length of received data");
    {
        let a = a0.association_internal.lock().await;
        assert_eq!(a.buffered_amount(), MSG.len(), "incorrect bufferedAmount");
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
        assert_eq!(a.buffered_amount(), 0, "incorrect bufferedAmount");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

// NB: This is ignored on Windows due to flakiness with timing/IO interactions.
// TODO: Refactor this and other tests that are disabled for similar reason to not have such issues
#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn test_assoc_reliable_ordered_reordered() -> Result<()> {
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
        assert_eq!(a.buffered_amount(), 0, "incorrect bufferedAmount");
    }

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

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
async fn test_assoc_reliable_ordered_fragmented_then_defragmented() -> Result<()> {
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
    assert_eq!(n, sbufl.len(), "unexpected length of received data");

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
async fn test_assoc_reliable_unordered_fragmented_then_defragmented() -> Result<()> {
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
    assert_eq!(n, sbufl.len(), "unexpected length of received data");

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
async fn test_assoc_reliable_unordered_ordered() -> Result<()> {
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

    br.reorder_next_nwrites(0, 2);

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

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

// NB: This is ignored on Windows due to flakiness with timing/IO interactions.
// TODO: Refactor this and other tests that are disabled for similar reason to not have such issues
#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn test_assoc_reliable_retransmission() -> Result<()> {
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
    static MSG1: Bytes = Bytes::from_static(b"ABC");
    static MSG2: Bytes = Bytes::from_static(b"DEFG");

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
    assert_eq!(n, MSG1.len(), "unexpected length of received data");

    let n = s0
        .write_sctp(&MSG2, PayloadProtocolIdentifier::Binary)
        .await?;
    assert_eq!(n, MSG2.len(), "unexpected length of received data");

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
async fn test_assoc_reliable_short_buffer() -> Result<()> {
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
    static MSG: Bytes = Bytes::from_static(b"Hello");

    let (br, ca, cb) = Bridge::new(0, None, None);

    let (a0, mut a1) =
        create_new_association_pair(&br, Arc::new(ca), Arc::new(cb), AckMode::NoDelay, 0).await?;

    let (s0, s1) = establish_session_pair(&br, &a0, &mut a1, SI).await?;

    {
        let a = a0.association_internal.lock().await;
        assert_eq!(a.buffered_amount(), 0, "incorrect bufferedAmount");
    }

    let n = s0
        .write_sctp(&MSG, PayloadProtocolIdentifier::Binary)
        .await?;
    assert_eq!(n, MSG.len(), "unexpected length of received data");
    {
        let a = a0.association_internal.lock().await;
        assert_eq!(a.buffered_amount(), MSG.len(), "incorrect bufferedAmount");
    }

    flush_buffers(&br, &a0, &a1).await;

    let mut buf = vec![0u8; 3];
    let result = s1.read_sctp(&mut buf).await;
    assert!(result.is_err(), "expected error to be ErrShortBuffer");
    if let Err(err) = result {
        assert_eq!(
            err,
            Error::ErrShortBuffer { size: 3 },
            "expected error to be ErrShortBuffer"
        );
    }

    {
        let q = s0.reassembly_queue.lock().await;
        assert!(!q.is_readable(), "should no longer be readable");
    }

    {
        let a = a0.association_internal.lock().await;
        assert_eq!(a.buffered_amount(), 0, "incorrect bufferedAmount");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_unreliable_rexmit_ordered_no_fragment() -> Result<()> {
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

    br.drop_next_nwrites(0, 1); // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

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
async fn test_assoc_unreliable_rexmit_ordered_fragment() -> Result<()> {
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

    br.drop_next_nwrites(0, 1); // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

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
async fn test_assoc_unreliable_rexmit_unordered_no_fragment() -> Result<()> {
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

    br.drop_next_nwrites(0, 1); // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

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

// NB: This is ignored on Windows and macOS due to flakiness with timing/IO interactions.
// TODO: Refactor this and other tests that are disabled for similar reason to not have such issues
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[tokio::test]
async fn test_assoc_unreliable_rexmit_unordered_fragment() -> Result<()> {
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
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

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
            q.unordered.len(),
            0,
            "should be nothing in the unordered queue"
        );
        assert_eq!(
            q.unordered_chunks.len(),
            0,
            "should be nothing in the unorderedChunks list"
        );
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_unreliable_rexmit_timed_ordered() -> Result<()> {
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

    br.drop_next_nwrites(0, 1); // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

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
async fn test_assoc_unreliable_rexmit_timed_unordered() -> Result<()> {
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

    br.drop_next_nwrites(0, 1); // drop the first packet (second one should be sacked)

    sbuf[0..4].copy_from_slice(&0u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

    sbuf[0..4].copy_from_slice(&1u32.to_be_bytes());
    let n = s0
        .write_sctp(
            &Bytes::from(sbuf.clone()),
            PayloadProtocolIdentifier::Binary,
        )
        .await?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

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
            q.unordered.len(),
            0,
            "should be nothing in the unordered queue"
        );
        assert_eq!(
            q.unordered_chunks.len(),
            0,
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
async fn test_assoc_congestion_control_fast_retransmission() -> Result<()> {
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

    br.drop_next_nwrites(0, 1); // drop the first packet (second one should be sacked)

    for i in 0..4u32 {
        sbuf[0..4].copy_from_slice(&i.to_be_bytes());
        let n = s0
            .write_sctp(
                &Bytes::from(sbuf.clone()),
                PayloadProtocolIdentifier::Binary,
            )
            .await?;
        assert_eq!(n, sbuf.len(), "unexpected length of received data");
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

        assert_eq!(a.stats.get_num_fast_retrans(), 1, "should be 1");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_congestion_control_congestion_avoidance() -> Result<()> {
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
        assert_eq!(n, sbuf.len(), "unexpected length of received data");
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
            assert_eq!(n, sbuf.len(), "unexpected length of received data");
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
            b.stats.get_num_datas(),
            N_PACKETS_TO_SEND as u64,
            "packet count mismatch"
        );
        assert!(
            a.stats.get_num_sacks() <= N_PACKETS_TO_SEND as u64 / 2,
            "too many sacks"
        );
        assert_eq!(a.stats.get_num_t3timeouts(), 0, "should be no retransmit");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_assoc_congestion_control_slow_reader() -> Result<()> {
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
        assert_eq!(n, sbuf.len(), "unexpected length of received data");
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
            assert_eq!(n, sbuf.len(), "unexpected length of received data");
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
        s1.get_num_bytes_in_reassembly_queue().await,
        0,
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
async fn test_assoc_delayed_ack() -> Result<()> {
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
        )?;
    assert_eq!(n, sbuf.len(), "unexpected length of received data");

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
            assert_eq!(n, sbuf.len(), "unexpected length of received data");
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
        s1.get_num_bytes_in_reassembly_queue().await,
        0,
        "reassembly queue should be empty"
    );

    {
        let a = a0.association_internal.lock().await;
        let b = a1.association_internal.lock().await;

        log::debug!("nDATAs      : {}", b.stats.get_num_datas());
        log::debug!("nSACKs      : {}", a.stats.get_num_sacks());
        log::debug!("nAckTimeouts: {}", b.stats.get_num_ack_timeouts());

        assert_eq!(b.stats.get_num_datas(), 1, "DATA chunk count mismatch");
        assert_eq!(
            a.stats.get_num_sacks(),
            b.stats.get_num_datas(),
            "sack count should be equal to the number of data chunks"
        );
        assert_eq!(
            b.stats.get_num_ack_timeouts(),
            1,
            "ackTimeout count mismatch"
        );
        assert_eq!(a.stats.get_num_t3timeouts(), 0, "should be no retransmit");
    }

    close_association_pair(&br, a0, a1).await;

    Ok(())
}
*/

//use std::io::Write;

#[tokio::test]
async fn test_assoc_reset_close_one_way() -> Result<()> {
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
    static MSG: Bytes = Bytes::from_static(b"ABC");

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
    assert_eq!(n, MSG.len(), "unexpected length of received data");
    {
        let a = a0.association_internal.lock().await;
        assert_eq!(a.buffered_amount(), MSG.len(), "incorrect bufferedAmount");
    }

    log::debug!("s0.shutdown");
    s0.shutdown(Shutdown::Both).await?; // send reset

    let (done_ch_tx, mut done_ch_rx) = mpsc::channel(1);
    let mut buf = vec![0u8; 32];

    tokio::spawn(async move {
        loop {
            log::debug!("s1.read_sctp begin");
            match s1.read_sctp(&mut buf).await {
                Ok((0, PayloadProtocolIdentifier::Unknown)) => {
                    log::debug!("s1.read_sctp EOF");
                    let _ = done_ch_tx.send(Some(Error::ErrEof)).await;
                    break;
                }
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
                    if err_opt.is_some() {
                        break;
                    }
                } else {
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
async fn test_assoc_reset_close_both_ways() -> Result<()> {
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
    static MSG: Bytes = Bytes::from_static(b"ABC");

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
    assert_eq!(n, MSG.len(), "unexpected length of received data");
    {
        let a = a0.association_internal.lock().await;
        assert_eq!(a.buffered_amount(), MSG.len(), "incorrect bufferedAmount");
    }

    log::debug!("s0.shutdown");
    s0.shutdown(Shutdown::Both).await?; // send reset

    let (done_ch_tx, mut done_ch_rx) = mpsc::channel(1);
    let done_ch_tx = Arc::new(done_ch_tx);

    let done_ch_tx1 = Arc::clone(&done_ch_tx);
    let ss1 = Arc::clone(&s1);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 32];
        loop {
            log::debug!("s1.read_sctp begin");
            match ss1.read_sctp(&mut buf).await {
                Ok((0, PayloadProtocolIdentifier::Unknown)) => {
                    log::debug!("s1.read_sctp EOF");
                    let _ = done_ch_tx1.send(Some(Error::ErrEof)).await;
                    break;
                }
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
                    if err_opt.is_some() {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }

    log::debug!("s1.shutdown");
    s1.shutdown(Shutdown::Both).await?; // send reset

    let done_ch_tx0 = Arc::clone(&done_ch_tx);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 32];

        log::debug!("s.read_sctp begin");
        match s0.read_sctp(&mut buf).await {
            Ok((0, PayloadProtocolIdentifier::Unknown)) => {
                log::debug!("s0.read_sctp EOF");
                let _ = done_ch_tx0.send(Some(Error::ErrEof)).await;
            }
            Ok(_) => {
                panic!("must be error");
            }
            Err(err) => {
                log::debug!("s0.read_sctp err {:?}", err);
                let _ = done_ch_tx0.send(Some(err)).await;
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
                    if err_opt.is_some() {
                        break;
                    } else {
                        panic!("must be error");
                    }
                } else {
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
async fn test_assoc_abort() -> Result<()> {
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
    assert_eq!(a0.get_state(), AssociationState::Established);
    assert_eq!(a1.get_state(), AssociationState::Established);

    let result = a0.net_conn.send(&packet).await;
    assert!(result.is_ok(), "must be ok");

    flush_buffers(&br, &a0, &a1).await;

    // There is a little delay before changing the state to closed
    tokio::time::sleep(Duration::from_millis(10)).await;

    // The receiving association should be closed because it got an ABORT
    assert_eq!(a0.get_state(), AssociationState::Established);
    assert_eq!(a1.get_state(), AssociationState::Closed);

    close_association_pair(&br, a0, a1).await;

    Ok(())
}

struct FakeEchoConn {
    wr_tx: Mutex<mpsc::Sender<Vec<u8>>>,
    rd_rx: Mutex<mpsc::Receiver<Vec<u8>>>,
    bytes_sent: AtomicUsize,
    bytes_received: AtomicUsize,
}

impl FakeEchoConn {
    fn type_erased() -> impl Conn {
        Self::default()
    }
}

impl Default for FakeEchoConn {
    fn default() -> Self {
        let (wr_tx, rd_rx) = mpsc::channel(1);
        FakeEchoConn {
            wr_tx: Mutex::new(wr_tx),
            rd_rx: Mutex::new(rd_rx),
            bytes_sent: AtomicUsize::new(0),
            bytes_received: AtomicUsize::new(0),
        }
    }
}

type UResult<T> = std::result::Result<T, util::Error>;

#[async_trait]
impl Conn for FakeEchoConn {
    async fn connect(&self, _addr: SocketAddr) -> UResult<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    async fn recv(&self, b: &mut [u8]) -> UResult<usize> {
        let mut rd_rx = self.rd_rx.lock().await;
        let v = match rd_rx.recv().await {
            Some(v) => v,
            None => {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Unexpected EOF").into())
            }
        };
        let l = std::cmp::min(v.len(), b.len());
        b[..l].copy_from_slice(&v[..l]);
        self.bytes_received.fetch_add(l, Ordering::SeqCst);
        Ok(l)
    }

    async fn recv_from(&self, _buf: &mut [u8]) -> UResult<(usize, SocketAddr)> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    async fn send(&self, b: &[u8]) -> UResult<usize> {
        let wr_tx = self.wr_tx.lock().await;
        match wr_tx.send(b.to_vec()).await {
            Ok(_) => {}
            Err(err) => return Err(io::Error::new(io::ErrorKind::Other, err.to_string()).into()),
        };
        self.bytes_sent.fetch_add(b.len(), Ordering::SeqCst);
        Ok(b.len())
    }

    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> UResult<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Not applicable").into())
    }

    fn local_addr(&self) -> UResult<SocketAddr> {
        Err(io::Error::new(io::ErrorKind::AddrNotAvailable, "Addr Not Available").into())
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        None
    }

    async fn close(&self) -> UResult<()> {
        Ok(())
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

//use std::io::Write;

#[tokio::test]
async fn test_stats() -> Result<()> {
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

    let conn = Arc::new(FakeEchoConn::type_erased());
    let a = Association::client(Config {
        net_conn: Arc::clone(&conn) as Arc<dyn Conn + Send + Sync>,
        max_receive_buffer_size: 0,
        max_message_size: 0,
        name: "client".to_owned(),
    })
    .await?;

    if let Some(conn) = conn.as_any().downcast_ref::<FakeEchoConn>() {
        assert_eq!(
            conn.bytes_received.load(Ordering::SeqCst),
            a.bytes_received()
        );
        assert_eq!(conn.bytes_sent.load(Ordering::SeqCst), a.bytes_sent());
    } else {
        panic!("must be FakeEchoConn");
    }

    Ok(())
}

async fn create_assocs() -> Result<(Association, Association)> {
    let addr1 = SocketAddr::from_str("0.0.0.0:0").unwrap();
    let addr2 = SocketAddr::from_str("0.0.0.0:0").unwrap();

    let udp1 = UdpSocket::bind(addr1).await.unwrap();
    let udp2 = UdpSocket::bind(addr2).await.unwrap();

    udp1.connect(udp2.local_addr().unwrap()).await.unwrap();
    udp2.connect(udp1.local_addr().unwrap()).await.unwrap();

    let (a1chan_tx, mut a1chan_rx) = mpsc::channel(1);
    let (a2chan_tx, mut a2chan_rx) = mpsc::channel(1);

    tokio::spawn(async move {
        let a = Association::client(Config {
            net_conn: Arc::new(udp1),
            max_receive_buffer_size: 0,
            max_message_size: 0,
            name: "client".to_owned(),
        })
        .await?;

        let _ = a1chan_tx.send(a).await;

        Result::<()>::Ok(())
    });

    tokio::spawn(async move {
        let a = Association::server(Config {
            net_conn: Arc::new(udp2),
            max_receive_buffer_size: 0,
            max_message_size: 0,
            name: "server".to_owned(),
        })
        .await?;

        let _ = a2chan_tx.send(a).await;

        Result::<()>::Ok(())
    });

    let timer1 = tokio::time::sleep(Duration::from_secs(1));
    tokio::pin!(timer1);
    let a1 = tokio::select! {
        _ = timer1.as_mut() =>{
            panic!("timed out waiting for a1");
        },
        a1 = a1chan_rx.recv() => {
            a1.unwrap()
        }
    };

    let timer2 = tokio::time::sleep(Duration::from_secs(1));
    tokio::pin!(timer2);
    let a2 = tokio::select! {
        _ = timer2.as_mut() =>{
            panic!("timed out waiting for a2");
        },
        a2 = a2chan_rx.recv() => {
            a2.unwrap()
        }
    };

    Ok((a1, a2))
}

//use std::io::Write;
//TODO: remove this conditional test
#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn test_association_shutdown() -> Result<()> {
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

    let (a1, a2) = create_assocs().await?;

    let s11 = a1.open_stream(1, PayloadProtocolIdentifier::String).await?;
    let s21 = a2.open_stream(1, PayloadProtocolIdentifier::String).await?;

    let test_data = Bytes::from_static(b"test");

    let n = s11.write(&test_data).await?;
    assert_eq!(n, test_data.len());

    let mut buf = vec![0u8; test_data.len()];
    let n = s21.read(&mut buf).await?;
    assert_eq!(n, test_data.len());
    assert_eq!(&buf[0..n], &test_data);

    if let Ok(result) = tokio::time::timeout(Duration::from_secs(1), a1.shutdown()).await {
        assert!(result.is_ok(), "shutdown should be ok");
    } else {
        panic!("shutdown timeout");
    }

    {
        let mut close_loop_ch_rx = a2.close_loop_ch_rx.lock().await;

        // Wait for close read loop channels to prevent flaky tests.
        let timer2 = tokio::time::sleep(Duration::from_secs(1));
        tokio::pin!(timer2);
        tokio::select! {
            _ = timer2.as_mut() =>{
                panic!("timed out waiting for a2 read loop to close");
            },
            _ = close_loop_ch_rx.recv() => {
                log::debug!("recv a2.close_loop_ch_rx");
            }
        };
    }
    Ok(())
}

//use std::io::Write;
//TODO: remove this conditional test
#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn test_association_shutdown_during_write() -> Result<()> {
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

    let (a1, a2) = create_assocs().await?;

    let s11 = a1.open_stream(1, PayloadProtocolIdentifier::String).await?;
    let s21 = a2.open_stream(1, PayloadProtocolIdentifier::String).await?;

    let (writing_done_tx, mut writing_done_rx) = mpsc::channel::<()>(1);
    let ss21 = Arc::clone(&s21);
    tokio::spawn(async move {
        let mut i = 0;
        while ss21.write(&Bytes::from(vec![i])).await.is_ok() {
            if i == 255 {
                i = 0;
            } else {
                i += 1;
            }

            if i % 100 == 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }

        drop(writing_done_tx);
    });

    let test_data = Bytes::from_static(b"test");

    let n = s11.write(&test_data).await?;
    assert_eq!(n, test_data.len());

    let mut buf = vec![0u8; test_data.len()];
    let n = s21.read(&mut buf).await?;
    assert_eq!(n, test_data.len());
    assert_eq!(&buf[0..n], &test_data);

    {
        let mut close_loop_ch_rx = a1.close_loop_ch_rx.lock().await;
        tokio::select! {
            res = tokio::time::timeout(Duration::from_secs(1), a1.shutdown()) => {
                if let Ok(result) = res {
                    assert!(result.is_ok(), "shutdown should be ok");
                } else {
                    panic!("shutdown timeout");
                }
            }
            _ = writing_done_rx.recv() => {
                log::debug!("writing_done_rx");
                let result = close_loop_ch_rx.recv().await;
                log::debug!("a1.close_loop_ch_rx.recv: {:?}", result);
            },
        };
    }

    {
        let mut close_loop_ch_rx = a2.close_loop_ch_rx.lock().await;
        // Wait for close read loop channels to prevent flaky tests.
        let timer2 = tokio::time::sleep(Duration::from_secs(1));
        tokio::pin!(timer2);
        tokio::select! {
            _ = timer2.as_mut() =>{
                panic!("timed out waiting for a2 read loop to close");
            },
            _ = close_loop_ch_rx.recv() => {
                log::debug!("recv a2.close_loop_ch_rx");
            }
        };
    }

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_association_handle_packet_before_init() -> Result<()> {
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

    let tests = vec![
        (
            "InitAck",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::new(ChunkInit {
                    is_ack: true,
                    initiate_tag: 1,
                    num_inbound_streams: 1,
                    num_outbound_streams: 1,
                    advertised_receiver_window_credit: 1500,
                    ..Default::default()
                })],
            },
        ),
        (
            "Abort",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::<ChunkAbort>::default()],
            },
        ),
        (
            "CoockeEcho",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::<ChunkCookieEcho>::default()],
            },
        ),
        (
            "HeartBeat",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::<ChunkHeartbeat>::default()],
            },
        ),
        (
            "PayloadData",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::<ChunkPayloadData>::default()],
            },
        ),
        (
            "Sack",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::new(ChunkSelectiveAck {
                    cumulative_tsn_ack: 1000,
                    advertised_receiver_window_credit: 1500,
                    gap_ack_blocks: vec![GapAckBlock {
                        start: 100,
                        end: 200,
                    }],
                    ..Default::default()
                })],
            },
        ),
        (
            "Reconfig",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::new(ChunkReconfig {
                    param_a: Some(Box::<ParamOutgoingResetRequest>::default()),
                    param_b: Some(Box::<ParamReconfigResponse>::default()),
                })],
            },
        ),
        (
            "ForwardTSN",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::new(ChunkForwardTsn {
                    new_cumulative_tsn: 100,
                    ..Default::default()
                })],
            },
        ),
        (
            "Error",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::<ChunkError>::default()],
            },
        ),
        (
            "Shutdown",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::<ChunkShutdown>::default()],
            },
        ),
        (
            "ShutdownAck",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::<ChunkShutdownAck>::default()],
            },
        ),
        (
            "ShutdownComplete",
            Packet {
                source_port: 1,
                destination_port: 1,
                verification_tag: 0,
                chunks: vec![Box::<ChunkShutdownComplete>::default()],
            },
        ),
    ];

    for (name, packet) in tests {
        log::debug!("testing {}", name);

        let (a_conn, charlie_conn) = pipe();

        let (a, _) = Association::new(
            Config {
                net_conn: Arc::new(a_conn),
                max_message_size: 0,
                max_receive_buffer_size: 0,
                name: "client".to_owned(),
            },
            true,
        )
        .await
        .unwrap();

        let packet = packet.marshal()?;
        let result = charlie_conn.send(&packet).await;
        assert!(result.is_ok(), "{name} charlie_conn.send should be ok");

        // Should not panic.
        tokio::time::sleep(Duration::from_millis(100)).await;

        a.close().await.unwrap();
    }

    Ok(())
}
