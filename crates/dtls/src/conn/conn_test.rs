use super::*;
use crate::cipher_suite::cipher_suite_aes_128_gcm_sha256::*;
use crate::cipher_suite::*;
use crate::compression_methods::*;
use crate::crypto::*;
use crate::curve::*;
use crate::errors::*;
use crate::extension::extension_supported_elliptic_curves::*;
use crate::extension::extension_supported_point_formats::*;
use crate::extension::extension_supported_signature_algorithms::*;
use crate::extension::*;
use crate::handshake::handshake_message_certificate::*;
use crate::handshake::handshake_message_client_hello::*;
use crate::handshake::handshake_message_hello_verify_request::*;
use crate::handshake::handshake_message_server_hello::*;
use crate::handshake::handshake_message_server_hello_done::*;
use crate::handshake::handshake_message_server_key_exchange::*;
use crate::handshake::handshake_random::*;
use crate::signature_hash_algorithm::*;

use rand::Rng;
use std::time::SystemTime;

use util::conn::conn_pipe::*;

//use std::io::Write;

lazy_static! {
    pub static ref ERR_TEST_PSK_INVALID_IDENTITY: Error =
        Error::new("TestPSK: Server got invalid identity".to_owned());
    pub static ref ERR_PSK_REJECTED: Error = Error::new("PSK Rejected".to_owned());
    pub static ref ERR_NOT_EXPECTED_CHAIN: Error = Error::new("not expected chain".to_owned());
    pub static ref ERR_EXPECTED_CHAIN: Error = Error::new("expected chain".to_owned());
    pub static ref ERR_WRONG_CERT: Error = Error::new("wrong cert".to_owned());
}

async fn build_pipe() -> Result<(Conn, Conn), Error> {
    let (ua, ub) = pipe();

    pipe_conn(Arc::new(ua), Arc::new(ub)).await
}

async fn pipe_conn(
    ca: Arc<dyn util::Conn + Send + Sync>,
    cb: Arc<dyn util::Conn + Send + Sync>,
) -> Result<(Conn, Conn), Error> {
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

fn psk_callback_hint_fail(_hint: &[u8]) -> Result<Vec<u8>, Error> {
    Err(ERR_PSK_REJECTED.clone())
}

async fn create_test_client(
    ca: Arc<dyn util::Conn + Send + Sync>,
    mut cfg: Config,
    generate_certificate: bool,
) -> Result<Conn, Error> {
    if generate_certificate {
        let client_cert = Certificate::generate_self_signed(vec!["localhost".to_owned()])?;
        cfg.certificates = vec![client_cert];
    }

    cfg.insecure_skip_verify = true;
    Conn::new(ca, cfg, true, None).await
}

async fn create_test_server(
    cb: Arc<dyn util::Conn + Send + Sync>,
    mut cfg: Config,
    generate_certificate: bool,
) -> Result<Conn, Error> {
    if generate_certificate {
        let server_cert = Certificate::generate_self_signed(vec!["localhost".to_owned()])?;
        cfg.certificates = vec![server_cert];
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

#[tokio::test]
async fn test_handshake_with_alert() -> Result<(), Error> {
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

        let (ca, cb) = pipe();
        tokio::spawn(async move {
            let result = create_test_client(Arc::new(ca), config_client, true).await;
            let _ = client_err_tx.send(result).await;
        });

        let result_server = create_test_server(Arc::new(cb), config_server, true).await;
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

#[tokio::test]
async fn test_export_keying_material() -> Result<(), Error> {
    let export_label = "EXTRACTOR-dtls_srtp";
    let expected_server_key = vec![0x61, 0x09, 0x9d, 0x7d, 0xcb, 0x08, 0x52, 0x2c, 0xe7, 0x7b];
    let expected_client_key = vec![0x87, 0xf0, 0x40, 0x02, 0xf6, 0x1c, 0xf1, 0xfe, 0x8c, 0x77];

    let (_decrypted_tx, decrypted_rx) = mpsc::channel(1);
    let (_handshake_tx, handshake_rx) = mpsc::channel(1);
    let (packet_tx, _packet_rx) = mpsc::channel(1);
    let (handle_queue_tx, _handle_queue_rx) = mpsc::channel(1);

    let mut c = Conn {
        state: State {
            local_random: HandshakeRandom {
                gmt_unix_time: SystemTime::UNIX_EPOCH
                    .checked_add(Duration::new(500, 0))
                    .unwrap(),
                ..Default::default()
            },
            remote_random: HandshakeRandom {
                gmt_unix_time: SystemTime::UNIX_EPOCH
                    .checked_add(Duration::new(1000, 0))
                    .unwrap(),
                ..Default::default()
            },
            local_sequence_number: Arc::new(Mutex::new(vec![0, 0])),
            cipher_suite: Arc::new(Mutex::new(Some(Box::new(CipherSuiteAes128GcmSha256::new(
                false,
            ))))),
            ..Default::default()
        },
        cache: HandshakeCache::new(),
        decrypted_rx,
        handshake_completed_successfully: Arc::new(AtomicBool::new(false)),
        connection_closed_by_user: false,
        closed: false,
        current_flight: Box::new(Flight0 {}) as Box<dyn Flight + Send + Sync>,
        flights: None,
        cfg: HandshakeConfig::default(),
        retransmit: false,
        handshake_rx,

        packet_tx: Arc::new(packet_tx),
        handle_queue_tx,
        handshake_done_tx: None,

        reader_close_tx: None,
    };

    c.set_local_epoch(0);
    let state = c.connection_state().await;
    if let Err(err) = state.export_keying_material(&export_label, &[], 0).await {
        assert_eq!(
            err,
            ERR_HANDSHAKE_IN_PROGRESS.clone(),
            "ExportKeyingMaterial when epoch == 0: expected '{}' actual '{}'",
            ERR_HANDSHAKE_IN_PROGRESS.clone(),
            err,
        );
    } else {
        assert!(false, "expect error but export_keying_material returns OK");
    }

    c.set_local_epoch(1);
    let state = c.connection_state().await;
    if let Err(err) = state
        .export_keying_material(&export_label, &[0x00], 0)
        .await
    {
        assert_eq!(
            err,
            ERR_CONTEXT_UNSUPPORTED.clone(),
            "ExportKeyingMaterial with context: expected '{}' actual '{}'",
            ERR_CONTEXT_UNSUPPORTED.clone(),
            err
        );
    } else {
        assert!(false, "expect error but export_keying_material returns OK");
    }

    for (k, _v) in INVALID_KEYING_LABELS.iter() {
        let state = c.connection_state().await;
        if let Err(err) = state.export_keying_material(k, &[], 0).await {
            assert_eq!(
                err,
                ERR_RESERVED_EXPORT_KEYING_MATERIAL.clone(),
                "ExportKeyingMaterial reserved label: expected '{}' actual '{}'",
                ERR_RESERVED_EXPORT_KEYING_MATERIAL.clone(),
                err,
            );
        } else {
            assert!(false, "expect error but export_keying_material returns OK");
        }
    }

    let state = c.connection_state().await;
    let keying_material = state.export_keying_material(&export_label, &[], 10).await?;
    assert_eq!(
        &keying_material, &expected_server_key,
        "ExportKeyingMaterial client export: expected ({:?}) actual ({:?})",
        &expected_server_key, &keying_material,
    );

    c.state.is_client = true;
    let state = c.connection_state().await;
    let keying_material = state.export_keying_material(&export_label, &[], 10).await?;
    assert_eq!(
        &keying_material, &expected_client_key,
        "ExportKeyingMaterial client export: expected ({:?}) actual ({:?})",
        &expected_client_key, &keying_material,
    );

    Ok(())
}

#[tokio::test]
async fn test_psk() -> Result<(), Error> {
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

    let tests = vec![
        (
            "Server identity specified",
            Some("Test Identity".as_bytes().to_vec()),
        ),
        ("Server identity nil", None),
    ];

    for (name, server_identity) in tests {
        let client_identity = "Client Identity".as_bytes();
        let (client_res_tx, mut client_res_rx) = mpsc::channel(1);

        let (ca, cb) = pipe();
        tokio::spawn(async move {
            let conf = Config {
                psk: Some(psk_callback_client),
                psk_identity_hint: Some(client_identity.to_vec()),
                cipher_suites: vec![CipherSuiteID::TLS_PSK_WITH_AES_128_CCM_8],
                ..Default::default()
            };

            let result = create_test_client(Arc::new(ca), conf, false).await;
            let _ = client_res_tx.send(result).await;
        });

        let config = Config {
            psk: Some(psk_callback_server),
            psk_identity_hint: server_identity,
            cipher_suites: vec![CipherSuiteID::TLS_PSK_WITH_AES_128_CCM_8],
            ..Default::default()
        };

        let mut server = create_test_server(Arc::new(cb), config, false).await?;

        if let Some(result) = client_res_rx.recv().await {
            if let Ok(mut client) = result {
                client.close().await?;
            } else {
                assert!(
                    false,
                    "{}: Expected create_test_client successfully, but got error",
                    name,
                );
            }
        }

        let _ = server.close().await;
    }

    Ok(())
}

#[tokio::test]
async fn test_psk_hint_fail() -> Result<(), Error> {
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

    let (client_res_tx, mut client_res_rx) = mpsc::channel(1);

    let (ca, cb) = pipe();
    tokio::spawn(async move {
        let conf = Config {
            psk: Some(psk_callback_hint_fail),
            psk_identity_hint: Some(vec![]),
            cipher_suites: vec![CipherSuiteID::TLS_PSK_WITH_AES_128_CCM_8],
            ..Default::default()
        };

        let result = create_test_client(Arc::new(ca), conf, false).await;
        let _ = client_res_tx.send(result).await;
    });

    let config = Config {
        psk: Some(psk_callback_hint_fail),
        psk_identity_hint: Some(vec![]),
        cipher_suites: vec![CipherSuiteID::TLS_PSK_WITH_AES_128_CCM_8],
        ..Default::default()
    };

    if let Err(server_err) = create_test_server(Arc::new(cb), config, false).await {
        assert_eq!(
            server_err,
            ERR_ALERT_FATAL_OR_CLOSE.clone(),
            "TestPSK: Server error exp({}) failed({})",
            ERR_ALERT_FATAL_OR_CLOSE.clone(),
            server_err,
        );
    } else {
        assert!(false, "Expected server error, but got OK");
    }

    let result = client_res_rx.recv().await;
    if let Some(client) = result {
        if let Err(client_err) = client {
            assert_eq!(
                client_err,
                ERR_PSK_REJECTED.clone(),
                "TestPSK: Client error exp({}) failed({})",
                ERR_PSK_REJECTED.clone(),
                client_err,
            );
        } else {
            assert!(false, "Expected client error, but got OK");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_client_timeout() -> Result<(), Error> {
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

    let (client_res_tx, mut client_res_rx) = mpsc::channel(1);

    let (ca, _cb) = pipe();
    tokio::spawn(async move {
        let conf = Config::default();
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            create_test_client(Arc::new(ca), conf, true),
        )
        .await;
        let _ = client_res_tx.send(result).await;
    });

    // no server!
    let result = client_res_rx.recv().await;
    if let Some(client_timeout_result) = result {
        if let Err(err) = client_timeout_result {
            assert!(
                true,
                "Client error exp(Temporary network error) failed({})",
                err
            );
        } else {
            assert!(false, "Expected Error but got Ok");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_srtp_configuration() -> Result<(), Error> {
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

    let tests = vec![
        (
            "No SRTP in use",
            vec![],
            vec![],
            SRTPProtectionProfile::Unsupported,
            None,
            None,
        ),
        (
            "SRTP both ends",
            vec![SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80],
            vec![SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80],
            SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80,
            None,
            None,
        ),
        (
            "SRTP client only",
            vec![SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80],
            vec![],
            SRTPProtectionProfile::Unsupported,
            Some(ERR_ALERT_FATAL_OR_CLOSE.clone()),
            Some(ERR_SERVER_NO_MATCHING_SRTP_PROFILE.clone()),
        ),
        (
            "SRTP server only",
            vec![],
            vec![SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80],
            SRTPProtectionProfile::Unsupported,
            None,
            None,
        ),
        (
            "Multiple Suites",
            vec![
                SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80,
                SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_32,
            ],
            vec![
                SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80,
                SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_32,
            ],
            SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80,
            None,
            None,
        ),
        (
            "Multiple Suites, Client Chooses",
            vec![
                SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80,
                SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_32,
            ],
            vec![
                SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_32,
                SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80,
            ],
            SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80,
            None,
            None,
        ),
    ];

    for (name, client_srtp, server_srtp, expected_profile, want_client_err, want_server_err) in
        tests
    {
        let (client_res_tx, mut client_res_rx) = mpsc::channel(1);
        let (ca, cb) = pipe();
        tokio::spawn(async move {
            let conf = Config {
                srtp_protection_profiles: client_srtp,
                ..Default::default()
            };

            let result = create_test_client(Arc::new(ca), conf, true).await;
            let _ = client_res_tx.send(result).await;
        });

        let config = Config {
            srtp_protection_profiles: server_srtp,
            ..Default::default()
        };

        let result = create_test_server(Arc::new(cb), config, true).await;
        if let Some(expected_err) = want_server_err {
            if let Err(err) = result {
                assert_eq!(
                    err, expected_err,
                    "{} TestPSK: Server error exp({}) failed({})",
                    name, expected_err, err,
                );
            } else {
                assert!(false, "{} expected error, but got ok", name);
            }
        } else {
            if let Ok(server) = result {
                let actual_server_srtp = server.selected_srtpprotection_profile();
                assert_eq!(actual_server_srtp, expected_profile,
                           "test_srtp_configuration: Server SRTPProtectionProfile Mismatch '{}': expected({:?}) actual({:?})",
                           name, expected_profile, actual_server_srtp);
            } else {
                assert!(false, "{} expected no error", name);
            }
        }

        let client_result = client_res_rx.recv().await;
        if let Some(result) = client_result {
            if let Some(expected_err) = want_client_err {
                if let Err(err) = result {
                    assert_eq!(
                        err, expected_err,
                        "TestPSK: Client error exp({}) failed({})",
                        expected_err, err,
                    );
                } else {
                    assert!(false, "{} expected error, but got ok", name);
                }
            } else {
                if let Ok(client) = result {
                    let actual_client_srtp = client.selected_srtpprotection_profile();
                    assert_eq!(actual_client_srtp, expected_profile,
                               "test_srtp_configuration: Client SRTPProtectionProfile Mismatch '{}': expected({:?}) actual({:?})",
                               name, expected_profile, actual_client_srtp);
                } else {
                    assert!(false, "{} expected no error", name);
                }
            }
        } else {
            assert!(false, "{} expected client, but got none", name);
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_client_certificate() -> Result<(), Error> {
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

    let server_name = "localhost".to_owned();

    let srv_cert = Certificate::generate_self_signed(vec!["localhost".to_owned()])?;
    let mut srv_ca_pool = rustls::RootCertStore::empty();
    srv_ca_pool
        .add(&srv_cert.certificate)
        .or_else(|_err| Err(Error::new("add srv_cert error".to_owned())))?;

    let cert = Certificate::generate_self_signed(vec!["localhost".to_owned()])?;
    let mut roots_cas = rustls::RootCertStore::empty();
    roots_cas
        .add(&cert.certificate)
        .or_else(|_err| Err(Error::new("add cert error".to_owned())))?;

    let tests = vec![
        (
            "NoClientCert",
            Config {
                roots_cas: srv_ca_pool.clone(),
                server_name: server_name.clone(),
                ..Default::default()
            },
            Config {
                certificates: vec![srv_cert.clone()],
                client_auth: ClientAuthType::NoClientCert,
                ..Default::default()
            },
            false,
        ),
        (
            "NoClientCert_cert",
            Config {
                roots_cas: srv_ca_pool.clone(),
                server_name: server_name.clone(),
                certificates: vec![cert.clone()],
                ..Default::default()
            },
            Config {
                certificates: vec![srv_cert.clone()],
                client_auth: ClientAuthType::RequireAnyClientCert,
                ..Default::default()
            },
            false,
        ),
        (
            "RequestClientCert_cert",
            Config {
                roots_cas: srv_ca_pool.clone(),
                server_name: server_name.clone(),
                certificates: vec![cert.clone()],
                ..Default::default()
            },
            Config {
                certificates: vec![srv_cert.clone()],
                client_auth: ClientAuthType::RequestClientCert,
                ..Default::default()
            },
            false,
        ),
        (
            "RequestClientCert_no_cert",
            Config {
                roots_cas: srv_ca_pool.clone(),
                server_name: server_name.clone(),
                ..Default::default()
            },
            Config {
                certificates: vec![srv_cert.clone()],
                client_auth: ClientAuthType::RequestClientCert,
                ..Default::default()
            },
            false,
        ),
        (
            "RequireAnyClientCert",
            Config {
                roots_cas: srv_ca_pool.clone(),
                server_name: server_name.clone(),
                certificates: vec![cert.clone()],
                ..Default::default()
            },
            Config {
                certificates: vec![srv_cert.clone()],
                client_auth: ClientAuthType::RequireAnyClientCert,
                ..Default::default()
            },
            false,
        ),
        (
            "RequireAnyClientCert_error",
            Config {
                roots_cas: srv_ca_pool.clone(),
                server_name: server_name.clone(),
                ..Default::default()
            },
            Config {
                certificates: vec![srv_cert.clone()],
                client_auth: ClientAuthType::RequireAnyClientCert,
                ..Default::default()
            },
            true,
        ),
        (
            "VerifyClientCertIfGiven_no_cert",
            Config {
                roots_cas: srv_ca_pool.clone(),
                server_name: server_name.clone(),
                ..Default::default()
            },
            Config {
                certificates: vec![srv_cert.clone()],
                client_auth: ClientAuthType::VerifyClientCertIfGiven,
                client_cert_verifier: Some(rustls::AllowAnyAuthenticatedClient::new(
                    roots_cas.clone(),
                )),
                ..Default::default()
            },
            false,
        ),
        (
            "VerifyClientCertIfGiven_cert",
            Config {
                roots_cas: srv_ca_pool.clone(),
                server_name: server_name.clone(),
                certificates: vec![cert.clone()],
                ..Default::default()
            },
            Config {
                certificates: vec![srv_cert.clone()],
                client_auth: ClientAuthType::VerifyClientCertIfGiven,
                client_cert_verifier: Some(rustls::AllowAnyAuthenticatedClient::new(
                    roots_cas.clone(),
                )),
                ..Default::default()
            },
            false,
        ),
        (
            "VerifyClientCertIfGiven_error",
            Config {
                roots_cas: srv_ca_pool.clone(),
                server_name: server_name.clone(),
                certificates: vec![cert.clone()],
                ..Default::default()
            },
            Config {
                certificates: vec![srv_cert.clone()],
                client_auth: ClientAuthType::VerifyClientCertIfGiven,
                ..Default::default()
            },
            true,
        ),
        (
            "RequireAndVerifyClientCert",
            Config {
                roots_cas: srv_ca_pool.clone(),
                server_name: server_name.clone(),
                certificates: vec![cert.clone()],
                ..Default::default()
            },
            Config {
                certificates: vec![srv_cert.clone()],
                client_auth: ClientAuthType::RequireAndVerifyClientCert,
                client_cert_verifier: Some(rustls::AllowAnyAuthenticatedClient::new(
                    roots_cas.clone(),
                )),
                ..Default::default()
            },
            false,
        ),
    ];

    for (name, client_cfg, server_cfg, want_err) in tests {
        let (client_res_tx, mut client_res_rx) = mpsc::channel(1);
        let (ca, cb) = pipe();
        let client_cfg_clone = client_cfg.clone();
        tokio::spawn(async move {
            let result = Conn::new(Arc::new(ca), client_cfg_clone, true, None).await;
            let _ = client_res_tx.send(result).await;
        });

        let result = Conn::new(Arc::new(cb), server_cfg.clone(), false, None).await;
        let client_result = client_res_rx.recv().await;

        if want_err {
            if result.is_err() {
                continue;
            }
            assert!(false, "{} Error expected", name);
        }

        assert!(
            result.is_ok(),
            "{} Server failed({:?})",
            name,
            result.err().unwrap()
        );
        assert!(client_result.is_some(), "{}, expected client conn", name);

        let res = client_result.unwrap();
        assert!(
            res.is_ok(),
            "{} Client failed({:?})",
            name,
            res.err().unwrap()
        );

        let server = result.unwrap();
        let client = res.unwrap();

        let actual_client_cert = &server.connection_state().await.peer_certificates;
        if server_cfg.client_auth == ClientAuthType::RequireAnyClientCert
            || server_cfg.client_auth == ClientAuthType::RequireAndVerifyClientCert
        {
            assert!(
                !actual_client_cert.is_empty(),
                "{} Client did not provide a certificate",
                name,
            );
            //if actual_client_cert.len() != len(tt.clientCfg.Certificates[0].Certificate) || !bytes.Equal(tt.clientCfg.Certificates[0].Certificate[0], actual_client_cert[0]) {
            assert_eq!(
                actual_client_cert[0],
                client_cfg.certificates[0].certificate.as_ref(),
                "{} Client certificate was not communicated correctly",
                name,
            );
        }

        if server_cfg.client_auth == ClientAuthType::NoClientCert {
            assert!(
                actual_client_cert.is_empty(),
                "{} Client certificate wasn't expected",
                name,
            );
        }

        let actual_server_cert = &client.connection_state().await.peer_certificates;
        assert!(
            !actual_server_cert.is_empty(),
            "{} Server did not provide a certificate",
            name,
        );

        /*if len(actual_server_cert) != len(tt.serverCfg.Certificates[0].Certificate)
        || !bytes.Equal(
            tt.serverCfg.Certificates[0].Certificate[0],
            actual_server_cert[0],
        )*/
        assert_eq!(
            actual_server_cert[0].len(),
            server_cfg.certificates[0].certificate.as_ref().len(),
            "{} Server certificate was not communicated correctly",
            name,
        );
        assert_eq!(
            actual_server_cert[0],
            server_cfg.certificates[0].certificate.as_ref(),
            "{} Server certificate was not communicated correctly",
            name,
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_extended_master_secret() -> Result<(), Error> {
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

    let tests = vec![
        (
            "Request_Request_ExtendedMasterSecret",
            Config {
                extended_master_secret: ExtendedMasterSecretType::Request,
                ..Default::default()
            },
            Config {
                extended_master_secret: ExtendedMasterSecretType::Request,
                ..Default::default()
            },
            None,
            None,
        ),
        (
            "Request_Require_ExtendedMasterSecret",
            Config {
                extended_master_secret: ExtendedMasterSecretType::Request,
                ..Default::default()
            },
            Config {
                extended_master_secret: ExtendedMasterSecretType::Require,
                ..Default::default()
            },
            None,
            None,
        ),
        (
            "Request_Disable_ExtendedMasterSecret",
            Config {
                extended_master_secret: ExtendedMasterSecretType::Request,
                ..Default::default()
            },
            Config {
                extended_master_secret: ExtendedMasterSecretType::Disable,
                ..Default::default()
            },
            None,
            None,
        ),
        (
            "Require_Request_ExtendedMasterSecret",
            Config {
                extended_master_secret: ExtendedMasterSecretType::Require,
                ..Default::default()
            },
            Config {
                extended_master_secret: ExtendedMasterSecretType::Request,
                ..Default::default()
            },
            None,
            None,
        ),
        (
            "Require_Require_ExtendedMasterSecret",
            Config {
                extended_master_secret: ExtendedMasterSecretType::Require,
                ..Default::default()
            },
            Config {
                extended_master_secret: ExtendedMasterSecretType::Require,
                ..Default::default()
            },
            None,
            None,
        ),
        (
            "Require_Disable_ExtendedMasterSecret",
            Config {
                extended_master_secret: ExtendedMasterSecretType::Require,
                ..Default::default()
            },
            Config {
                extended_master_secret: ExtendedMasterSecretType::Disable,
                ..Default::default()
            },
            Some(ERR_CLIENT_REQUIRED_BUT_NO_SERVER_EMS.clone()),
            Some(ERR_ALERT_FATAL_OR_CLOSE.clone()),
        ),
        (
            "Disable_Request_ExtendedMasterSecret",
            Config {
                extended_master_secret: ExtendedMasterSecretType::Disable,
                ..Default::default()
            },
            Config {
                extended_master_secret: ExtendedMasterSecretType::Request,
                ..Default::default()
            },
            None,
            None,
        ),
        (
            "Disable_Require_ExtendedMasterSecret",
            Config {
                extended_master_secret: ExtendedMasterSecretType::Disable,
                ..Default::default()
            },
            Config {
                extended_master_secret: ExtendedMasterSecretType::Require,
                ..Default::default()
            },
            Some(ERR_ALERT_FATAL_OR_CLOSE.clone()),
            Some(ERR_SERVER_REQUIRED_BUT_NO_CLIENT_EMS.clone()),
        ),
        (
            "Disable_Disable_ExtendedMasterSecret",
            Config {
                extended_master_secret: ExtendedMasterSecretType::Disable,
                ..Default::default()
            },
            Config {
                extended_master_secret: ExtendedMasterSecretType::Disable,
                ..Default::default()
            },
            None,
            None,
        ),
    ];

    for (name, client_cfg, server_cfg, expected_client_err, expected_server_err) in tests {
        let (client_res_tx, mut client_res_rx) = mpsc::channel(1);
        let (ca, cb) = pipe();
        let client_cfg_clone = client_cfg.clone();
        tokio::spawn(async move {
            let result = create_test_client(Arc::new(ca), client_cfg_clone, true).await;
            let _ = client_res_tx.send(result).await;
        });

        let result = create_test_server(Arc::new(cb), server_cfg.clone(), true).await;
        let client_result = client_res_rx.recv().await;
        assert!(client_result.is_some(), "{}, expected client conn", name);
        let res = client_result.unwrap();

        if let Some(client_err) = expected_client_err {
            if let Err(err) = res {
                assert_eq!(
                    err, client_err,
                    "Client error expected: \"{}\" but got \"{}\"",
                    client_err, err,
                );
            } else {
                assert!(false, "{} expected err, but got ok", name);
            }
        } else {
            assert!(res.is_ok(), "{} expected ok, but got err", name);
        }

        if let Some(server_err) = expected_server_err {
            if let Err(err) = result {
                assert_eq!(
                    err, server_err,
                    "Server error expected: \"{}\" but got \"{}\"",
                    server_err, err,
                );
            } else {
                assert!(false, "{} expected err, but got ok", name);
            }
        } else {
            assert!(result.is_ok(), "{} expected ok, but got err", name);
        }
    }

    Ok(())
}

fn fn_not_expected_chain(_cert: &[Vec<u8>], chain: &[rustls::Certificate]) -> Result<(), Error> {
    if !chain.is_empty() {
        return Err(ERR_NOT_EXPECTED_CHAIN.clone());
    }
    Ok(())
}

fn fn_expected_chain(_cert: &[Vec<u8>], chain: &[rustls::Certificate]) -> Result<(), Error> {
    if chain.is_empty() {
        return Err(ERR_EXPECTED_CHAIN.clone());
    }
    Ok(())
}

fn fn_wrong_cert(_cert: &[Vec<u8>], _chain: &[rustls::Certificate]) -> Result<(), Error> {
    Err(ERR_WRONG_CERT.clone())
}

#[tokio::test]
async fn test_server_certificate() -> Result<(), Error> {
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

    let server_name = "localhost".to_owned();
    let cert = Certificate::generate_self_signed(vec![server_name.clone()])?;
    let mut roots_cas = rustls::RootCertStore::empty();
    roots_cas
        .add(&cert.certificate)
        .or_else(|_err| Err(Error::new("add cert error".to_owned())))?;

    let tests = vec![
        (
            "no_ca",
            Config {
                server_name: server_name.clone(),
                ..Default::default()
            },
            Config {
                certificates: vec![cert.clone()],
                client_auth: ClientAuthType::NoClientCert,
                ..Default::default()
            },
            true,
        ),
        (
            "good_ca",
            Config {
                roots_cas: roots_cas.clone(),
                server_name: server_name.clone(),
                ..Default::default()
            },
            Config {
                certificates: vec![cert.clone()],
                client_auth: ClientAuthType::NoClientCert,
                ..Default::default()
            },
            false,
        ),
        (
            "no_ca_skip_verify",
            Config {
                insecure_skip_verify: true,
                server_name: server_name.clone(),
                ..Default::default()
            },
            Config {
                certificates: vec![cert.clone()],
                client_auth: ClientAuthType::NoClientCert,
                ..Default::default()
            },
            false,
        ),
        (
            "good_ca_skip_verify_custom_verify_peer",
            Config {
                roots_cas: roots_cas.clone(),
                server_name: server_name.clone(),
                certificates: vec![cert.clone()],
                ..Default::default()
            },
            Config {
                certificates: vec![cert.clone()],
                client_auth: ClientAuthType::RequireAnyClientCert,
                verify_peer_certificate: Some(fn_not_expected_chain),
                ..Default::default()
            },
            false,
        ),
        (
            "good_ca_verify_custom_verify_peer",
            Config {
                roots_cas: roots_cas.clone(),
                server_name: server_name.clone(),
                certificates: vec![cert.clone()],
                ..Default::default()
            },
            Config {
                certificates: vec![cert.clone()],
                client_auth: ClientAuthType::RequireAndVerifyClientCert,
                client_cert_verifier: Some(rustls::AllowAnyAuthenticatedClient::new(
                    roots_cas.clone(),
                )),
                verify_peer_certificate: Some(fn_expected_chain),
                ..Default::default()
            },
            false,
        ),
        (
            "good_ca_custom_verify_peer",
            Config {
                roots_cas: roots_cas.clone(),
                server_name: server_name.clone(),
                verify_peer_certificate: Some(fn_wrong_cert),
                ..Default::default()
            },
            Config {
                certificates: vec![cert.clone()],
                client_auth: ClientAuthType::NoClientCert,
                ..Default::default()
            },
            true,
        ),
        (
            "server_name",
            Config {
                roots_cas: roots_cas.clone(),
                server_name: server_name.clone(),
                ..Default::default()
            },
            Config {
                certificates: vec![cert.clone()],
                client_auth: ClientAuthType::NoClientCert,
                ..Default::default()
            },
            false,
        ),
        (
            "server_name_error",
            Config {
                roots_cas: roots_cas.clone(),
                server_name: "barfoo".to_owned(),
                ..Default::default()
            },
            Config {
                certificates: vec![cert.clone()],
                client_auth: ClientAuthType::NoClientCert,
                ..Default::default()
            },
            true,
        ),
    ];

    for (name, client_cfg, server_cfg, want_err) in tests {
        let (res_tx, mut res_rx) = mpsc::channel(1);
        let (ca, cb) = pipe();

        tokio::spawn(async move {
            let result = Conn::new(Arc::new(cb), server_cfg, false, None).await;
            let _ = res_tx.send(result).await;
        });

        let cli_result = Conn::new(Arc::new(ca), client_cfg, true, None).await;

        if !want_err && cli_result.is_err() {
            assert!(
                false,
                "{}: Client failed({})",
                name,
                cli_result.err().unwrap()
            );
        }
        if want_err && cli_result.is_ok() {
            assert!(false, "{}: Error expected", name);
        }

        let _ = res_rx.recv().await;
    }
    Ok(())
}

#[tokio::test]
async fn test_cipher_suite_configuration() -> Result<(), Error> {
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

    let tests = vec![
        (
            "No CipherSuites specified",
            vec![],
            vec![],
            None,
            None,
            None,
        ),
        (
            "Invalid CipherSuite",
            vec![CipherSuiteID::Unsupported],
            vec![CipherSuiteID::Unsupported],
            Some(ERR_INVALID_CIPHER_SUITE.clone()),
            Some(ERR_INVALID_CIPHER_SUITE.clone()),
            None,
        ),
        (
            "Valid CipherSuites specified",
            vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256],
            vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256],
            None,
            None,
            Some(CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256),
        ),
        (
            "CipherSuites mismatch",
            vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256],
            vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA],
            Some(ERR_ALERT_FATAL_OR_CLOSE.clone()),
            Some(ERR_CIPHER_SUITE_NO_INTERSECTION.clone()),
            None,
        ),
        (
            "Valid CipherSuites CCM specified",
            vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM],
            vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM],
            None,
            None,
            Some(CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM),
        ),
        (
            "Valid CipherSuites CCM-8 specified",
            vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8],
            vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8],
            None,
            None,
            Some(CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8),
        ),
        (
            "Server supports subset of client suites",
            vec![
                CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
                CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA,
            ],
            vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA],
            None,
            None,
            Some(CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA),
        ),
    ];

    for (
        name,
        client_cipher_suites,
        server_cipher_suites,
        want_client_error,
        want_server_error,
        want_selected_cipher_suite,
    ) in tests
    {
        let (client_res_tx, mut client_res_rx) = mpsc::channel(1);
        let (ca, cb) = pipe();
        tokio::spawn(async move {
            let conf = Config {
                cipher_suites: client_cipher_suites,
                ..Default::default()
            };

            let result = create_test_client(Arc::new(ca), conf, true).await;
            let _ = client_res_tx.send(result).await;
        });

        let config = Config {
            cipher_suites: server_cipher_suites,
            ..Default::default()
        };

        let result = create_test_server(Arc::new(cb), config, true).await;
        if let Some(expected_err) = want_server_error {
            if let Err(err) = result {
                assert_eq!(
                    err, expected_err,
                    "{} test_cipher_suite_configuration: Server error exp({}) failed({})",
                    name, expected_err, err,
                );
            } else {
                assert!(false, "{} expected error, but got ok", name);
            }
        } else {
            assert!(result.is_ok(), "{} expected ok, but got error", name)
        }

        let client_result = client_res_rx.recv().await;
        if let Some(result) = client_result {
            if let Some(expected_err) = want_client_error {
                if let Err(err) = result {
                    assert_eq!(
                        err, expected_err,
                        "{} test_cipher_suite_configuration: Client error exp({}) failed({})",
                        name, expected_err, err,
                    );
                } else {
                    assert!(false, "{} expected error, but got ok", name);
                }
            } else {
                assert!(result.is_ok(), "{} expected ok, but got error", name);
                let client = result.unwrap();
                if let Some(want_cs) = want_selected_cipher_suite {
                    let cipher_suite = client.state.cipher_suite.lock().await;
                    assert!(
                        cipher_suite.is_some(),
                        "{} expected some, but got none",
                        name
                    );
                    if let Some(cs) = &*cipher_suite {
                        assert_eq!(cs.id(), want_cs,
                                   "test_cipher_suite_configuration: Server Selected Bad Cipher Suite '{}': expected({}) actual({})", 
                                   name, want_cs, cs.id());
                    }
                }
            }
        } else {
            assert!(false, "{} expected Some, but got None", name);
        }
    }

    Ok(())
}

fn psk_callback(_b: &[u8]) -> Result<Vec<u8>, Error> {
    Ok(vec![0x00, 0x01, 0x02])
}

#[tokio::test]
async fn test_psk_configuration() -> Result<(), Error> {
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

    let tests = vec![
        (
            "PSK specified",
            false,
            false,
            true, //Some(psk_callback),
            true, //Some(psk_callback),
            Some(vec![0x00]),
            Some(vec![0x00]),
            Some(ERR_NO_AVAILABLE_CIPHER_SUITES.clone()),
            Some(ERR_NO_AVAILABLE_CIPHER_SUITES.clone()),
        ),
        (
            "PSK and certificate specified",
            true,
            true,
            true, //Some(psk_callback),
            true, //Some(psk_callback),
            Some(vec![0x00]),
            Some(vec![0x00]),
            Some(ERR_PSK_AND_CERTIFICATE.clone()),
            Some(ERR_PSK_AND_CERTIFICATE.clone()),
        ),
        (
            "PSK and no identity specified",
            false,
            false,
            true, //Some(psk_callback),
            true, //Some(psk_callback),
            None,
            None,
            Some(ERR_PSK_AND_IDENTITY_MUST_BE_SET_FOR_CLIENT.clone()),
            Some(ERR_NO_AVAILABLE_CIPHER_SUITES.clone()),
        ),
        (
            "No PSK and identity specified",
            false,
            false,
            false,
            false,
            Some(vec![0x00]),
            Some(vec![0x00]),
            Some(ERR_IDENTITY_NO_PSK.clone()),
            Some(ERR_SERVER_MUST_HAVE_CERTIFICATE.clone()),
        ),
    ];

    for (
        name,
        client_has_certificate,
        server_has_certificate,
        client_psk,
        server_psk,
        client_psk_identity,
        server_psk_identity,
        want_client_error,
        want_server_error,
    ) in tests
    {
        let (client_res_tx, mut client_res_rx) = mpsc::channel(1);
        let (ca, cb) = pipe();
        tokio::spawn(async move {
            let conf = Config {
                psk: if client_psk { Some(psk_callback) } else { None },
                psk_identity_hint: client_psk_identity,
                ..Default::default()
            };

            let result = create_test_client(Arc::new(ca), conf, client_has_certificate).await;
            let _ = client_res_tx.send(result).await;
        });

        let config = Config {
            psk: if server_psk { Some(psk_callback) } else { None },
            psk_identity_hint: server_psk_identity,
            ..Default::default()
        };

        let result = create_test_server(Arc::new(cb), config, server_has_certificate).await;
        if let Some(expected_err) = want_server_error {
            if let Err(err) = result {
                assert_eq!(
                    err, expected_err,
                    "{} test_psk_configuration: Server error exp({}) failed({})",
                    name, expected_err, err,
                );
            } else {
                assert!(false, "{} expected error, but got ok", name);
            }
        } else {
            assert!(result.is_ok(), "{} expected ok, but got error", name)
        }

        let client_result = client_res_rx.recv().await;
        if let Some(result) = client_result {
            if let Some(expected_err) = want_client_error {
                if let Err(err) = result {
                    assert_eq!(
                        err, expected_err,
                        "{} test_psk_configuration: Client error exp({}) failed({})",
                        name, expected_err, err,
                    );
                } else {
                    assert!(false, "{} expected error, but got ok", name);
                }
            } else {
                assert!(result.is_ok(), "{} expected ok, but got error", name);
            }
        } else {
            assert!(false, "{} expected Some, but got None", name);
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_server_timeout() -> Result<(), Error> {
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

    let mut cookie = vec![0u8; 20];
    rand::thread_rng().fill(cookie.as_mut_slice());

    let random_bytes = [0u8; RANDOM_BYTES_LENGTH];
    let gmt_unix_time = SystemTime::UNIX_EPOCH
        .checked_add(Duration::new(500, 0))
        .unwrap();
    let random = HandshakeRandom {
        gmt_unix_time,
        random_bytes,
    };

    let cipher_suites = vec![
        CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256, //&cipherSuiteTLSEcdheEcdsaWithAes128GcmSha256{},
        CipherSuiteID::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256, //&cipherSuiteTLSEcdheRsaWithAes128GcmSha256{},
    ];

    let extensions = vec![
        Extension::SupportedSignatureAlgorithms(ExtensionSupportedSignatureAlgorithms {
            signature_hash_algorithms: vec![
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA256,
                    signature: SignatureAlgorithm::ECDSA,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA384,
                    signature: SignatureAlgorithm::ECDSA,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA512,
                    signature: SignatureAlgorithm::ECDSA,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA256,
                    signature: SignatureAlgorithm::RSA,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA384,
                    signature: SignatureAlgorithm::RSA,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::SHA512,
                    signature: SignatureAlgorithm::RSA,
                },
            ],
        }),
        Extension::SupportedEllipticCurves(ExtensionSupportedEllipticCurves {
            elliptic_curves: vec![NamedCurve::X25519, NamedCurve::P256, NamedCurve::P384],
        }),
        Extension::SupportedPointFormats(ExtensionSupportedPointFormats {
            point_formats: vec![ELLIPTIC_CURVE_POINT_FORMAT_UNCOMPRESSED],
        }),
    ];

    let record = RecordLayer::new(
        PROTOCOL_VERSION1_2,
        0,
        Content::Handshake(Handshake::new(HandshakeMessage::ClientHello(
            HandshakeMessageClientHello {
                version: PROTOCOL_VERSION1_2,
                cookie,
                random,
                cipher_suites,
                compression_methods: default_compression_methods(),
                extensions,
            },
        ))),
    );

    let mut packet = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(packet.as_mut());
        record.marshal(&mut writer)?;
    }

    use util::Conn;
    let (ca, cb) = pipe();

    // Client reader
    let (ca_read_chan_tx, mut ca_read_chan_rx) = mpsc::channel(1000);

    let ca_rx = Arc::new(ca);
    let ca_tx = Arc::clone(&ca_rx);

    tokio::spawn(async move {
        let mut data = vec![0; 8192];
        loop {
            if let Ok(n) = ca_rx.recv(&mut data).await {
                let result = ca_read_chan_tx.send(data[..n].to_vec()).await;
                if result.is_ok() {
                    return;
                }
            } else {
                return;
            }
        }
    });

    // Start sending ClientHello packets until server responds with first packet
    tokio::spawn(async move {
        loop {
            let timer = tokio::time::sleep(Duration::from_millis(10));
            tokio::pin!(timer);

            tokio::select! {
                _ = timer.as_mut() => {
                    let result = ca_tx.send(&packet).await;
                    if result.is_err() {
                        return;
                    }
                }
                _ = ca_read_chan_rx.recv() => return,
            }
        }
    });

    let config = Config {
        cipher_suites: vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256],
        flight_interval: Duration::from_millis(100),
        ..Default::default()
    };

    let result = tokio::time::timeout(
        Duration::from_millis(50),
        create_test_server(Arc::new(cb), config, true),
    )
    .await;
    if let Err(err) = result {
        assert!(
            true,
            "Sever error exp(Temporary network error) failed({})",
            err
        );
    } else {
        assert!(false, "Expected Error but got Ok");
    }

    // Wait a little longer to ensure no additional messages have been sent by the server
    //tokio::time::sleep(Duration::from_millis(300)).await;

    /*tokio::select! {
    case msg := <-caReadChan:
        t.Fatalf("Expected no additional messages from server, got: %+v", msg)
    default:
    }*/

    Ok(())
}

#[tokio::test]
async fn test_protocol_version_validation() -> Result<(), Error> {
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

    let mut cookie = vec![0; 20];
    rand::thread_rng().fill(cookie.as_mut_slice());

    let random_bytes = [0u8; RANDOM_BYTES_LENGTH];
    let gmt_unix_time = SystemTime::UNIX_EPOCH
        .checked_add(Duration::new(500, 0))
        .unwrap();
    let random = HandshakeRandom {
        gmt_unix_time,
        random_bytes,
    };

    let local_keypair = NamedCurve::X25519.generate_keypair()?;

    //|"Server"|
    {
        let server_cases = vec![
            (
                "ClientHelloVersion",
                vec![RecordLayer::new(
                    PROTOCOL_VERSION1_2,
                    0,
                    Content::Handshake(Handshake::new(HandshakeMessage::ClientHello(
                        HandshakeMessageClientHello {
                            version: ProtocolVersion {
                                major: 0xfe,
                                minor: 0xff,
                            }, // try to downgrade
                            cookie: cookie.clone(),
                            random: random.clone(),
                            cipher_suites: vec![
                                CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
                            ],
                            compression_methods: default_compression_methods(),
                            extensions: vec![],
                        },
                    ))),
                )],
            ),
            (
                "SecondsClientHelloVersion",
                vec![
                    RecordLayer::new(
                        PROTOCOL_VERSION1_2,
                        0,
                        Content::Handshake(Handshake::new(HandshakeMessage::ClientHello(
                            HandshakeMessageClientHello {
                                version: PROTOCOL_VERSION1_2,
                                cookie: cookie.clone(),
                                random: random.clone(),
                                cipher_suites: vec![
                                    CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
                                ],
                                compression_methods: default_compression_methods(),
                                extensions: vec![],
                            },
                        ))),
                    ),
                    {
                        let mut handshake = Handshake::new(HandshakeMessage::ClientHello(
                            HandshakeMessageClientHello {
                                version: ProtocolVersion {
                                    major: 0xfe,
                                    minor: 0xff,
                                }, // try to downgrade
                                cookie: cookie.clone(),
                                random: random.clone(),
                                cipher_suites: vec![
                                    CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
                                ],
                                compression_methods: default_compression_methods(),
                                extensions: vec![],
                            },
                        ));
                        handshake.handshake_header.message_sequence = 1;
                        let mut record_layer =
                            RecordLayer::new(PROTOCOL_VERSION1_2, 0, Content::Handshake(handshake));
                        record_layer.record_layer_header.sequence_number = 1;

                        record_layer
                    },
                ],
            ),
        ];

        use util::Conn;
        for (name, records) in server_cases {
            let (ca, cb) = pipe();

            tokio::spawn(async move {
                let config = Config {
                    cipher_suites: vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256],
                    flight_interval: Duration::from_millis(100),
                    ..Default::default()
                };
                let timeout_result = tokio::time::timeout(
                    Duration::from_millis(1000),
                    create_test_server(Arc::new(cb), config, true),
                )
                .await;
                match timeout_result {
                    Ok(result) => {
                        if let Err(err) = result {
                            assert_eq!(
                                err,
                                ERR_UNSUPPORTED_PROTOCOL_VERSION.clone(),
                                "{} Client error exp({}) failed({})",
                                name,
                                ERR_UNSUPPORTED_PROTOCOL_VERSION.clone(),
                                err,
                            );
                        } else {
                            assert!(false, "{} expected error, but got ok", name);
                        }
                    }
                    Err(err) => {
                        assert!(false, "server timeout {}", err);
                    }
                };
            });

            tokio::time::sleep(Duration::from_millis(50)).await;

            let mut resp = vec![0; 1024];
            let mut n = 0;
            for record in records {
                let mut packet = vec![];
                {
                    let mut writer = BufWriter::<&mut Vec<u8>>::new(packet.as_mut());
                    record.marshal(&mut writer)?;
                }

                let _ = ca.send(&packet).await;
                n = ca.recv(&mut resp).await?;
            }

            let mut reader = BufReader::new(&resp[..n]);
            let h = RecordLayerHeader::unmarshal(&mut reader)?;
            assert_eq!(
                h.content_type,
                ContentType::Alert,
                "Peer must return alert to unsupported protocol version"
            );
        }
    }

    //"Client"
    {
        let client_cases = vec![(
            "ServerHelloVersion",
            vec![
                RecordLayer::new(
                    PROTOCOL_VERSION1_2,
                    0,
                    Content::Handshake(Handshake::new(HandshakeMessage::HelloVerifyRequest(
                        HandshakeMessageHelloVerifyRequest {
                            version: PROTOCOL_VERSION1_2,
                            cookie: cookie.clone(),
                        },
                    ))),
                ),
                {
                    let mut handshake = Handshake::new(HandshakeMessage::ServerHello(
                        HandshakeMessageServerHello {
                            version: ProtocolVersion {
                                major: 0xfe,
                                minor: 0xff,
                            }, // try to downgrade
                            random: random.clone(),
                            cipher_suite: CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
                            compression_method: default_compression_methods().ids[0],
                            extensions: vec![],
                        },
                    ));
                    handshake.handshake_header.message_sequence = 1;
                    let mut record =
                        RecordLayer::new(PROTOCOL_VERSION1_2, 0, Content::Handshake(handshake));
                    record.record_layer_header.sequence_number = 1;
                    record
                },
                {
                    let mut handshake = Handshake::new(HandshakeMessage::Certificate(
                        HandshakeMessageCertificate {
                            certificate: vec![],
                        },
                    ));
                    handshake.handshake_header.message_sequence = 2;
                    let mut record =
                        RecordLayer::new(PROTOCOL_VERSION1_2, 0, Content::Handshake(handshake));
                    record.record_layer_header.sequence_number = 2;
                    record
                },
                {
                    let mut handshake = Handshake::new(HandshakeMessage::ServerKeyExchange(
                        HandshakeMessageServerKeyExchange {
                            identity_hint: vec![],
                            elliptic_curve_type: EllipticCurveType::NamedCurve,
                            named_curve: NamedCurve::X25519,
                            public_key: local_keypair.public_key.clone(),
                            hash_algorithm: HashAlgorithm::SHA256,
                            signature_algorithm: SignatureAlgorithm::ECDSA,
                            signature: vec![0; 64],
                        },
                    ));
                    handshake.handshake_header.message_sequence = 3;
                    let mut record =
                        RecordLayer::new(PROTOCOL_VERSION1_2, 0, Content::Handshake(handshake));
                    record.record_layer_header.sequence_number = 3;
                    record
                },
                {
                    let mut handshake = Handshake::new(HandshakeMessage::ServerHelloDone(
                        HandshakeMessageServerHelloDone {},
                    ));
                    handshake.handshake_header.message_sequence = 4;
                    let mut record =
                        RecordLayer::new(PROTOCOL_VERSION1_2, 0, Content::Handshake(handshake));
                    record.record_layer_header.sequence_number = 4;
                    record
                },
            ],
        )];

        use util::Conn;
        for (name, records) in client_cases {
            let (ca, cb) = pipe();

            tokio::spawn(async move {
                let config = Config {
                    cipher_suites: vec![CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256],
                    flight_interval: Duration::from_millis(100),
                    ..Default::default()
                };
                let timeout_result = tokio::time::timeout(
                    Duration::from_millis(1000),
                    create_test_client(Arc::new(cb), config, true),
                )
                .await;
                match timeout_result {
                    Ok(result) => {
                        if let Err(err) = result {
                            assert_eq!(
                                err,
                                ERR_UNSUPPORTED_PROTOCOL_VERSION.clone(),
                                "{} Server error exp({}) failed({})",
                                name,
                                ERR_UNSUPPORTED_PROTOCOL_VERSION.clone(),
                                err,
                            );
                        } else {
                            assert!(false, "{} expected error, but got ok", name);
                        }
                    }
                    Err(err) => {
                        assert!(false, "server timeout {}", err);
                    }
                };
            });

            tokio::time::sleep(Duration::from_millis(50)).await;

            let mut resp = vec![0; 1024];
            for record in records {
                let _ = ca.recv(&mut resp).await?;

                let mut packet = vec![];
                {
                    let mut writer = BufWriter::<&mut Vec<u8>>::new(packet.as_mut());
                    record.marshal(&mut writer)?;
                }
                let _ = ca.send(&packet).await;
            }

            let n = ca.recv(&mut resp).await?;

            let mut reader = BufReader::new(&resp[..n]);
            let h = RecordLayerHeader::unmarshal(&mut reader)?;

            assert_eq!(
                h.content_type,
                ContentType::Alert,
                "Peer must return alert to unsupported protocol version"
            );
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_multiple_hello_verify_request() -> Result<(), Error> {
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

    let mut cookies = vec![
        // first clientHello contains an empty cookie
        vec![],
    ];

    let mut packets = vec![];
    for i in 0..2 {
        let mut cookie = vec![0; 20];
        rand::thread_rng().fill(cookie.as_mut_slice());
        cookies.push(cookie.clone());

        let mut handshake = Handshake::new(HandshakeMessage::HelloVerifyRequest(
            HandshakeMessageHelloVerifyRequest {
                version: PROTOCOL_VERSION1_2,
                cookie,
            },
        ));
        handshake.handshake_header.message_sequence = i as u16;

        let mut record = RecordLayer::new(PROTOCOL_VERSION1_2, 0, Content::Handshake(handshake));
        record.record_layer_header.sequence_number = i as u64;

        let mut packet = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(packet.as_mut());
            record.marshal(&mut writer)?;
        }

        packets.push(packet);
    }

    use util::Conn;
    let (ca, cb) = pipe();

    tokio::spawn(async move {
        let conf = Config::default();
        let _ = tokio::time::timeout(
            Duration::from_millis(100),
            create_test_client(Arc::new(ca), conf, true),
        )
        .await;
    });

    for i in 0..cookies.len() {
        let cookie = &cookies[i];
        trace!("cookie {}: {:?}", i, cookie);

        // read client hello
        let mut resp = vec![0; 1024];
        let n = cb.recv(&mut resp).await?;
        let mut reader = BufReader::new(&resp[..n]);
        let record = RecordLayer::unmarshal(&mut reader)?;
        match record.content {
            Content::Handshake(h) => match h.handshake_message {
                HandshakeMessage::ClientHello(client_hello) => {
                    assert_eq!(
                        &client_hello.cookie, cookie,
                        "Wrong cookie {}, expected: {:?}, got: {:?}",
                        i, &client_hello.cookie, cookie
                    );
                }
                _ => assert!(false, "unexpected handshake message"),
            },
            _ => assert!(false, "unexpected content"),
        };

        if packets.len() <= i {
            break;
        }
        // write hello verify request
        cb.send(&packets[i]).await?;
    }

    Ok(())
}
