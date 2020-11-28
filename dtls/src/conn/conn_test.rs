use super::*;

use tokio::net::UdpSocket;

async fn build_pipe() -> Result<(Conn, Conn), Error> {
    let ua = UdpSocket::bind("127.0.0.1:0").await?;
    let ub = UdpSocket::bind("127.0.0.1:0").await?;

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
            true,
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
        true,
    )
    .await?;

    // Receive client
    let client = match c_rx.recv().await.unwrap() {
        Ok(client) => client,
        Err(err) => return Err(err),
    };

    Ok((client, sever))
}

async fn create_test_client(
    ca: UdpSocket,
    mut cfg: Config,
    generate_certificate: bool,
) -> Result<Conn, Error> {
    if generate_certificate {
        //TODO:
    }

    cfg.insecure_skip_verify = true;
    Conn::new(ca, cfg, true, None).await
}

async fn create_test_server(
    cb: UdpSocket,
    cfg: Config,
    generate_certificate: bool,
) -> Result<Conn, Error> {
    if generate_certificate {
        //TODO:
    }
    Conn::new(cb, cfg, false, None).await
}

#[tokio::test]
async fn test_routine_leak_on_close() -> Result<(), Error> {
    env_logger::init();

    /*let (mut ca, _cb) = build_pipe().await?;

    let n = ca.write(&[0; 100], Some(Duration::from_secs(5))).await?;
    assert_eq!(n, 100);*/

    Ok(())
}
