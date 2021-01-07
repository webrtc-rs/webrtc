use super::*;

#[tokio::test]
async fn test_valid_communication() -> Result<(), Error> {
    /*use std::io::Write;
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
        .init();*/

    let sock_a = UdpSocket::bind("0.0.0.0:0").await?;
    trace!("sock_a.local_addr={:?}", sock_a.local_addr());
    let port_a = sock_a.local_addr()?.port();

    let sock_b = UdpSocket::bind("0.0.0.0:0").await?;
    trace!("sock_b.local_addr={:?}", sock_b.local_addr());
    let port_b = sock_b.local_addr()?.port();

    let server_a = DNSConn::server(
        sock_a,
        Config {
            dst_port: Some(port_b),
            local_names: vec![
                "webrtc-rs-mdns-1.local".to_owned(),
                "webrtc-rs-mdns-2.local".to_owned(),
            ],
            ..Default::default()
        },
    )?;

    let server_b = DNSConn::server(
        sock_b,
        Config {
            dst_port: Some(port_a),
            ..Default::default()
        },
    )?;

    let res = tokio::time::timeout(
        Duration::from_millis(500),
        server_b.query("webrtc-rs-mdns-1.local"),
    )
    .await;
    assert!(!res.is_err(), "first server_b.query timeout!");

    let res = tokio::time::timeout(
        Duration::from_millis(500),
        server_b.query("webrtc-rs-mdns-2.local"),
    )
    .await;
    assert!(!res.is_err(), "second server_b.query timeout!");

    server_a.close().await?;
    server_b.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_multiple_close() -> Result<(), Error> {
    /*use std::io::Write;
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
        .init();*/

    let sock_a = UdpSocket::bind("0.0.0.0:0").await?;
    trace!("sock_a.local_addr={:?}", sock_a.local_addr());

    let server_a = DNSConn::server(sock_a, Config::default())?;

    server_a.close().await?;
    if let Err(err) = server_a.close().await {
        assert_eq!(err, *ERR_CONNECTION_CLOSED);
    } else {
        assert!(false, "expected error, but got ok");
    }

    Ok(())
}

#[tokio::test]
async fn test_query_respect_timeout() -> Result<(), Error> {
    /*use std::io::Write;
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
        .init();*/

    let sock_a = UdpSocket::bind("0.0.0.0:0").await?;
    trace!("sock_a.local_addr={:?}", sock_a.local_addr());

    let server_a = DNSConn::server(sock_a, Config::default())?;

    let res =
        tokio::time::timeout(Duration::from_millis(100), server_a.query("invalid-host")).await;
    assert!(res.is_err(), "server_a.query expects timeout!");

    server_a.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_query_respect_close() -> Result<(), Error> {
    /*use std::io::Write;
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
        .init();*/

    let sock_a = UdpSocket::bind("0.0.0.0:0").await?;
    trace!("sock_a.local_addr={:?}", sock_a.local_addr());

    let server = DNSConn::server(sock_a, Config::default())?;
    let server_a = Arc::new(server);
    let server_b = Arc::clone(&server_a);

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let _ = server_a.close().await;
    });

    if let Err(err) = server_b.query("invalid-host").await {
        assert_eq!(err, *ERR_CONNECTION_CLOSED);
    } else {
        assert!(false, "expected error, but got ok");
    }

    if let Err(err) = server_b.query("invalid-host").await {
        assert_eq!(err, *ERR_CONNECTION_CLOSED);
    } else {
        assert!(false, "expected error, but got ok");
    }

    Ok(())
}
