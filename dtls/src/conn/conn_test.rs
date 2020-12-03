use super::*;

use tokio::net::UdpSocket;

//use std::io::Write;

async fn build_pipe() -> Result<(Conn, Conn), Error> {
    let ua = UdpSocket::bind("127.0.0.1:0").await?;
    let ub = UdpSocket::bind("127.0.0.1:0").await?;

    trace!("{} vs {}", ua.local_addr()?, ub.local_addr()?);

    ua.connect(ub.local_addr()?).await?;
    ub.connect(ua.local_addr()?).await?;

    pipe_conn(ua, ub).await
}

async fn pipe_conn(ca: UdpSocket, cb: UdpSocket) -> Result<(Conn, Conn), Error> {
    let (c_tx, mut c_rx) = mpsc::channel(1);

    // Setup client
    tokio::spawn(async move {
        let client = create_test_client(
            ca,
            Config {
                srtp_protection_profiles: vec![SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80],
                ..Default::default()
            },
            false,
        )
        .await;

        let _ = c_tx.send(client).await;
    });

    // Setup server
    let sever = create_test_server(
        cb,
        Config {
            srtp_protection_profiles: vec![SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80],
            ..Default::default()
        },
        false,
    )
    .await?;

    // Receive client
    let client = match c_rx.recv().await.unwrap() {
        Ok(client) => client,
        Err(err) => return Err(err),
    };

    Ok((client, sever))
}

fn psk_callback_client(hint: &[u8]) -> Result<Vec<u8>, Error> {
    trace!(
        "Server's hint: {}",
        String::from_utf8(hint.to_vec()).unwrap()
    );
    Ok(vec![0xAB, 0xC1, 0x23])
}

fn psk_callback_server(hint: &[u8]) -> Result<Vec<u8>, Error> {
    trace!(
        "Client's hint: {}",
        String::from_utf8(hint.to_vec()).unwrap()
    );
    Ok(vec![0xAB, 0xC1, 0x23])
}

async fn create_test_client(
    ca: UdpSocket,
    mut cfg: Config,
    generate_certificate: bool,
) -> Result<Conn, Error> {
    if generate_certificate {
        //TODO:
    } else {
        cfg.psk = Some(psk_callback_client);
        cfg.psk_identity_hint = "WebRTC.rs DTLS Server".as_bytes().to_vec();
        cfg.cipher_suites = vec![CipherSuiteID::TLS_PSK_WITH_AES_128_GCM_SHA256];
    }

    cfg.insecure_skip_verify = true;
    Conn::new(ca, cfg, true, None).await
}

async fn create_test_server(
    cb: UdpSocket,
    mut cfg: Config,
    generate_certificate: bool,
) -> Result<Conn, Error> {
    if generate_certificate {
        //TODO:
    } else {
        cfg.psk = Some(psk_callback_server);
        cfg.psk_identity_hint = "WebRTC.rs DTLS Client".as_bytes().to_vec();
        cfg.cipher_suites = vec![CipherSuiteID::TLS_PSK_WITH_AES_128_GCM_SHA256];
    }
    Conn::new(cb, cfg, false, None).await
}

#[tokio::test]
async fn test_routine_leak_on_close() -> Result<(), Error> {
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

    let (mut ca, mut cb) = build_pipe().await?;

    let buf_a = vec![0xFA; 100];
    let n_a = ca.write(&buf_a, Some(Duration::from_secs(5))).await?;
    assert_eq!(n_a, 100);

    let mut buf_b = vec![0; 1024];
    let n_b = cb.read(&mut buf_b, None).await?;
    assert_eq!(n_a, 100);
    assert_eq!(&buf_a[..], &buf_b[0..n_b]);

    cb.close().await?;
    ca.close().await?;

    //tokio::time::sleep(Duration::from_secs(1)).await;

    Ok(())
}
