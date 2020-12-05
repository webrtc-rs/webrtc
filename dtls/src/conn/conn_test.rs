use super::*;
use crate::cipher_suite::*;
use crate::compression_methods::*;
use crate::handshake::handshake_message_client_hello::*;
use crate::handshake::handshake_random::*;
//use crate::signature_hash_algorithm::*;

use tokio::net::UdpSocket;

//use std::io::Write;

async fn build_pipe() -> Result<(Conn, Conn), Error> {
    let (ua, ub) = pipe().await?;

    pipe_conn(ua, ub).await
}

async fn pipe() -> Result<(UdpSocket, UdpSocket), Error> {
    let ua = UdpSocket::bind("127.0.0.1:0").await?;
    let ub = UdpSocket::bind("127.0.0.1:0").await?;

    trace!("{} vs {}", ua.local_addr()?, ub.local_addr()?);

    ua.connect(ub.local_addr()?).await?;
    ub.connect(ua.local_addr()?).await?;

    Ok((ua, ub))
}

async fn pipe_conn(ca: UdpSocket, cb: UdpSocket) -> Result<(Conn, Conn), Error> {
    let (c_tx, mut c_rx) = mpsc::channel(1);

    // Setup client
    tokio::spawn(async move {
        let client = create_test_client(
            ca,
            Config {
                srtp_protection_profiles: vec![SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80],
                cipher_suites: vec![CipherSuiteID::TLS_PSK_WITH_AES_128_GCM_SHA256],
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
            cipher_suites: vec![CipherSuiteID::TLS_PSK_WITH_AES_128_GCM_SHA256],
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
    let n_b = cb.read(&mut buf_b, Some(Duration::from_secs(5))).await?;
    assert_eq!(n_a, 100);
    assert_eq!(&buf_a[..], &buf_b[0..n_b]);

    cb.close().await?;
    ca.close().await?;

    {
        drop(ca);
        drop(cb);
    }

    tokio::time::sleep(Duration::from_millis(1)).await;

    Ok(())
}

#[tokio::test]
async fn test_sequence_number_overflow_on_application_data() -> Result<(), Error> {
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

    {
        let mut lsn = ca.state.local_sequence_number.lock().await;
        lsn[1] = MAX_SEQUENCE_NUMBER;
    }

    let buf_a = vec![0xFA; 100];
    let n_a = ca.write(&buf_a, Some(Duration::from_secs(5))).await?;
    assert_eq!(n_a, 100);

    let mut buf_b = vec![0; 1024];
    let n_b = cb.read(&mut buf_b, Some(Duration::from_secs(5))).await?;
    assert_eq!(n_a, 100);
    assert_eq!(&buf_a[..], &buf_b[0..n_b]);

    let result = ca.write(&buf_a, Some(Duration::from_secs(5))).await;
    if let Err(err) = result {
        assert_eq!(err, ERR_SEQUENCE_NUMBER_OVERFLOW.clone());
    } else {
        assert!(false, "Expected error but it is OK");
    }

    cb.close().await?;

    if let Err(err) = ca.close().await {
        assert_eq!(err, ERR_SEQUENCE_NUMBER_OVERFLOW.clone());
    } else {
        assert!(false, "Expected error but it is OK");
    }

    {
        drop(ca);
        drop(cb);
    }

    tokio::time::sleep(Duration::from_millis(1)).await;

    Ok(())
}

#[tokio::test]
async fn test_sequence_number_overflow_on_handshake() -> Result<(), Error> {
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

    {
        let mut lsn = ca.state.local_sequence_number.lock().await;
        lsn[0] = MAX_SEQUENCE_NUMBER + 1;
    }

    // Try to send handshake packet.
    if let Err(err) = ca
        .write_packets(vec![Packet {
            record: RecordLayer::new(
                PROTOCOL_VERSION1_2,
                0,
                Content::Handshake(Handshake::new(HandshakeMessage::ClientHello(
                    HandshakeMessageClientHello {
                        version: PROTOCOL_VERSION1_2,
                        random: HandshakeRandom::default(),
                        cookie: vec![0; 64],

                        cipher_suites: vec![CipherSuiteID::TLS_PSK_WITH_AES_128_GCM_SHA256],
                        compression_methods: default_compression_methods(),
                        extensions: vec![],
                    },
                ))),
            ),
            should_encrypt: false,
            reset_local_sequence_number: false,
        }])
        .await
    {
        assert_eq!(err, ERR_SEQUENCE_NUMBER_OVERFLOW.clone());
    } else {
        assert!(false, "Expected error but it is OK");
    }

    cb.close().await?;
    ca.close().await?;

    {
        drop(ca);
        drop(cb);
    }

    tokio::time::sleep(Duration::from_millis(1)).await;

    Ok(())
}

// TODO: enable it when self-sign is supported.
// https://github.com/webrtc-rs/webrtc/issues/25
/*
#[tokio::test]
async fn test_handshake_with_alert() -> Result<(), Error> {
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

    let cases = vec![
        (
            "CipherSuiteNoIntersection",
            Config {
                // Server
                cipher_suites: vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256],
                ..Default::default()
            },
            Config {
                // Client
                cipher_suites: vec![CipherSuiteID::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256],
                ..Default::default()
            },
            ERR_CIPHER_SUITE_NO_INTERSECTION.clone(),
            ERR_ALERT_FATAL_OR_CLOSE.clone(), //errClient: &errAlert{&alert{alertLevelFatal, alertInsufficientSecurity}},
        ),
        (
            "SignatureSchemesNoIntersection",
            Config {
                // Server
                cipher_suites: vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256],
                signature_schemes: vec![SignatureScheme::ECDSAWithP256AndSHA256],
                ..Default::default()
            },
            Config {
                // Client
                cipher_suites: vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256],
                signature_schemes: vec![SignatureScheme::ECDSAWithP521AndSHA512],
                ..Default::default()
            },
            ERR_ALERT_FATAL_OR_CLOSE.clone(), //errServer: &errAlert{&alert{alertLevelFatal, alertInsufficientSecurity}},
            ERR_NO_AVAILABLE_SIGNATURE_SCHEMES.clone(), //NoAvailableSignatureSchemes,
        ),
    ];

    for (name, config_server, config_client, err_server, err_client) in cases {
        let (client_err_tx, mut client_err_rx) = mpsc::channel(1);

        let (ca, cb) = pipe().await?;
        tokio::spawn(async move {
            let result = create_test_client(ca, config_client, false).await;
            let _ = client_err_tx.send(result).await;
        });

        let result_server = create_test_server(cb, config_server, false).await;
        if let Err(err) = result_server {
            assert_eq!(
                err, err_server,
                "{} Server error exp({}) failed({})",
                name, err_server, err
            );
        } else {
            assert!(
                false,
                "{} expected error but create_test_server return OK",
                name
            );
        }

        let result_client = client_err_rx.recv().await;
        if let Some(result_client) = result_client {
            if let Err(err) = result_client {
                assert_eq!(
                    err, err_client,
                    "{} Client error exp({}) failed({})",
                    name, err_client, err
                );
            } else {
                assert!(
                    false,
                    "{} expected error but create_test_client return OK",
                    name
                );
            }
        }
    }

    Ok(())
}
*/
