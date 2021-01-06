use super::*;

#[tokio::test]
async fn test_valid_communication() -> Result<(), Error> {
    /*
    use std::io::Write;

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

    let sock_a = UdpSocket::bind("0.0.0.0:0").await?;
    trace!("sock_a.local_addr={:?}", sock_a.local_addr());
    let sock_b = UdpSocket::bind("0.0.0.0:0").await?;
    trace!("sock_b.local_addr={:?}", sock_b.local_addr());

    let mut server_a = DNSConn::server(
        sock_a,
        Config {
            local_names: vec![
                "pion-mdns-1.local".to_owned(),
                "pion-mdns-2.local".to_owned(),
            ],
            ..Default::default()
        },
    )?;

    let mut server_b = DNSConn::server(sock_b, Config::default())?;

    let res =
        tokio::time::timeout(Duration::from_secs(1), server_b.query("pion-mdns-1.local")).await;
    assert!(!res.is_err(), "first server_b.query timeout!");

    let res =
        tokio::time::timeout(Duration::from_secs(1), server_b.query("pion-mdns-2.local")).await;
    assert!(!res.is_err(), "second server_b.query timeout!");

    server_a.close()?;
    server_b.close()?;
    */
    Ok(())
}
