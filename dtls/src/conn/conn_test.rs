use std::time::SystemTime;

use rand::Rng;
use rustls::pki_types::CertificateDer;
use util::conn::conn_pipe::*;
use util::KeyingMaterialExporter;

use super::*;
use crate::cipher_suite::cipher_suite_aes_128_gcm_sha256::*;
use crate::cipher_suite::*;
use crate::compression_methods::*;
use crate::crypto::*;
use crate::curve::*;
use crate::error::*;
use crate::extension::extension_supported_elliptic_curves::*;
use crate::extension::extension_supported_point_formats::*;
use crate::extension::extension_supported_signature_algorithms::*;
use crate::extension::renegotiation_info::ExtensionRenegotiationInfo;
use crate::extension::*;
use crate::handshake::handshake_message_certificate::*;
use crate::handshake::handshake_message_client_hello::*;
use crate::handshake::handshake_message_hello_verify_request::*;
use crate::handshake::handshake_message_server_hello::*;
use crate::handshake::handshake_message_server_hello_done::*;
use crate::handshake::handshake_message_server_key_exchange::*;
use crate::handshake::handshake_random::*;
use crate::signature_hash_algorithm::*;

const ERR_TEST_PSK_INVALID_IDENTITY: &str = "TestPSK: Server got invalid identity";
const ERR_PSK_REJECTED: &str = "PSK Rejected";
const ERR_NOT_EXPECTED_CHAIN: &str = "not expected chain";
const ERR_EXPECTED_CHAIN: &str = "expected chain";
const ERR_WRONG_CERT: &str = "wrong cert";

async fn build_pipe() -> Result<(DTLSConn, DTLSConn)> {
    let (ua, ub) = pipe();

    pipe_conn(Arc::new(ua), Arc::new(ub)).await
}

async fn pipe_conn(
    ca: Arc<dyn util::Conn + Send + Sync>,
    cb: Arc<dyn util::Conn + Send + Sync>,
) -> Result<(DTLSConn, DTLSConn)> {
    let (c_tx, mut c_rx) = mpsc::channel(1);

    // Setup client
    tokio::spawn(async move {
        let client = create_test_client(
            ca,
            Config {
                srtp_protection_profiles: vec![SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80],
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
            srtp_protection_profiles: vec![SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80],
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

fn psk_callback_client(hint: &[u8]) -> Result<Vec<u8>> {
    trace!(
        "Server's hint: {}",
        String::from_utf8(hint.to_vec()).unwrap()
    );
    Ok(vec![0xAB, 0xC1, 0x23])
}

fn psk_callback_server(hint: &[u8]) -> Result<Vec<u8>> {
    trace!(
        "Client's hint: {}",
        String::from_utf8(hint.to_vec()).unwrap()
    );
    Ok(vec![0xAB, 0xC1, 0x23])
}

fn psk_callback_hint_fail(_hint: &[u8]) -> Result<Vec<u8>> {
    Err(Error::Other(ERR_PSK_REJECTED.to_owned()))
}

async fn create_test_client(
    ca: Arc<dyn util::Conn + Send + Sync>,
    mut cfg: Config,
    generate_certificate: bool,
) -> Result<DTLSConn> {
    if generate_certificate {
        let client_cert = Certificate::generate_self_signed(vec!["localhost".to_owned()])?;
        cfg.certificates = vec![client_cert];
    }

    cfg.insecure_skip_verify = true;
    DTLSConn::new(ca, cfg, true, None).await
}

async fn create_test_server(
    cb: Arc<dyn util::Conn + Send + Sync>,
    mut cfg: Config,
    generate_certificate: bool,
) -> Result<DTLSConn> {
    if generate_certificate {
        let server_cert = Certificate::generate_self_signed(vec!["localhost".to_owned()])?;
        cfg.certificates = vec![server_cert];
    }

    DTLSConn::new(cb, cfg, false, None).await
}

#[tokio::test]
async fn test_routine_leak_on_close() -> Result<()> {
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

    let (ca, cb) = build_pipe().await?;

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
async fn test_sequence_number_overflow_on_application_data() -> Result<()> {
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

    let (ca, cb) = build_pipe().await?;

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
        assert_eq!(
            err.to_string(),
            Error::ErrSequenceNumberOverflow.to_string()
        );
    } else {
        panic!("Expected error but it is OK");
    }

    cb.close().await?;

    if let Err(err) = ca.close().await {
        assert_eq!(
            err.to_string(),
            Error::ErrSequenceNumberOverflow.to_string()
        );
    } else {
        panic!("Expected error but it is OK");
    }

    {
        drop(ca);
        drop(cb);
    }

    tokio::time::sleep(Duration::from_millis(1)).await;

    Ok(())
}

#[tokio::test]
async fn test_sequence_number_overflow_on_handshake() -> Result<()> {
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

    let (ca, cb) = build_pipe().await?;

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

                        cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256],
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
        assert_eq!(
            err.to_string(),
            Error::ErrSequenceNumberOverflow.to_string()
        );
    } else {
        panic!("Expected error but it is OK");
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
async fn test_handshake_with_alert() -> Result<()> {
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
                cipher_suites: vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256],
                ..Default::default()
            },
            Config {
                // Client
                cipher_suites: vec![CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256],
                ..Default::default()
            },
            Error::ErrCipherSuiteNoIntersection,
            Error::ErrAlertFatalOrClose, //errClient: &errAlert{&alert{alertLevelFatal, alertInsufficientSecurity}},
        ),
        (
            "SignatureSchemesNoIntersection",
            Config {
                // Server
                cipher_suites: vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256],
                signature_schemes: vec![SignatureScheme::EcdsaWithP256AndSha256],
                ..Default::default()
            },
            Config {
                // Client
                cipher_suites: vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256],
                signature_schemes: vec![SignatureScheme::EcdsaWithP521AndSha512],
                ..Default::default()
            },
            Error::ErrAlertFatalOrClose, //errServer: &errAlert{&alert{alertLevelFatal, alertInsufficientSecurity}},
            Error::ErrNoAvailableSignatureSchemes, //NoAvailableSignatureSchemes,
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
                err.to_string(),
                err_server.to_string(),
                "{name} Server error exp({err_server}) failed({err})"
            );
        } else {
            panic!("{name} expected error but create_test_server return OK");
        }

        let result_client = client_err_rx.recv().await;
        if let Some(result_client) = result_client {
            if let Err(err) = result_client {
                assert_eq!(
                    err.to_string(),
                    err_client.to_string(),
                    "{name} Client error exp({err_client}) failed({err})"
                );
            } else {
                panic!("{name} expected error but create_test_client return OK");
            }
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_export_keying_material() -> Result<()> {
    let export_label = "EXTRACTOR-dtls_srtp";
    let expected_server_key = vec![0x61, 0x09, 0x9d, 0x7d, 0xcb, 0x08, 0x52, 0x2c, 0xe7, 0x7b];
    let expected_client_key = vec![0x87, 0xf0, 0x40, 0x02, 0xf6, 0x1c, 0xf1, 0xfe, 0x8c, 0x77];

    let (_decrypted_tx, decrypted_rx) = mpsc::channel(1);
    let (_handshake_tx, handshake_rx) = mpsc::channel(1);
    let (packet_tx, _packet_rx) = mpsc::channel(1);
    let (handle_queue_tx, _handle_queue_rx) = mpsc::channel(1);
    let (ca, _cb) = pipe();

    let mut c = DTLSConn {
        conn: Arc::new(ca),
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
        decrypted_rx: Mutex::new(decrypted_rx),
        handshake_completed_successfully: Arc::new(AtomicBool::new(false)),
        connection_closed_by_user: false,
        closed: AtomicBool::new(false),
        current_flight: Box::new(Flight0 {}) as Box<dyn Flight + Send + Sync>,
        flights: None,
        cfg: HandshakeConfig::default(),
        retransmit: false,
        handshake_rx,

        packet_tx: Arc::new(packet_tx),
        handle_queue_tx,
        handshake_done_tx: None,

        reader_close_tx: Mutex::new(None),
    };

    c.set_local_epoch(0);
    let state = c.connection_state().await;
    if let Err(err) = state.export_keying_material(export_label, &[], 0).await {
        assert!(
            err.to_string()
                .contains(&Error::ErrHandshakeInProgress.to_string()),
            "ExportKeyingMaterial when epoch == 0: expected '{}' actual '{}'",
            Error::ErrHandshakeInProgress,
            err,
        );
    } else {
        panic!("expect error but export_keying_material returns OK");
    }

    c.set_local_epoch(1);
    let state = c.connection_state().await;
    if let Err(err) = state.export_keying_material(export_label, &[0x00], 0).await {
        assert!(
            err.to_string()
                .contains(&Error::ErrContextUnsupported.to_string()),
            "ExportKeyingMaterial with context: expected '{}' actual '{}'",
            Error::ErrContextUnsupported,
            err
        );
    } else {
        panic!("expect error but export_keying_material returns OK");
    }

    for k in INVALID_KEYING_LABELS.iter() {
        let state = c.connection_state().await;
        if let Err(err) = state.export_keying_material(k, &[], 0).await {
            assert!(
                err.to_string()
                    .contains(&Error::ErrReservedExportKeyingMaterial.to_string()),
                "ExportKeyingMaterial reserved label: expected '{}' actual '{}'",
                Error::ErrReservedExportKeyingMaterial,
                err,
            );
        } else {
            panic!("expect error but export_keying_material returns OK");
        }
    }

    let state = c.connection_state().await;
    let keying_material = state.export_keying_material(export_label, &[], 10).await?;
    assert_eq!(
        &keying_material, &expected_server_key,
        "ExportKeyingMaterial client export: expected ({:?}) actual ({:?})",
        &expected_server_key, &keying_material,
    );

    c.state.is_client = true;
    let state = c.connection_state().await;
    let keying_material = state.export_keying_material(export_label, &[], 10).await?;
    assert_eq!(
        &keying_material, &expected_client_key,
        "ExportKeyingMaterial client export: expected ({:?}) actual ({:?})",
        &expected_client_key, &keying_material,
    );

    Ok(())
}

#[tokio::test]
async fn test_psk() -> Result<()> {
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
                psk: Some(Arc::new(psk_callback_client)),
                psk_identity_hint: Some(client_identity.to_vec()),
                cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8],
                ..Default::default()
            };

            let result = create_test_client(Arc::new(ca), conf, false).await;
            let _ = client_res_tx.send(result).await;
        });

        let config = Config {
            psk: Some(Arc::new(psk_callback_server)),
            psk_identity_hint: server_identity,
            cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8],
            ..Default::default()
        };

        let server = create_test_server(Arc::new(cb), config, false).await?;

        let actual_psk_identity_hint = &server.connection_state().await.identity_hint;
        assert_eq!(
            actual_psk_identity_hint, client_identity,
            "TestPSK: Server ClientPSKIdentity Mismatch '{name}': expected({client_identity:?}) actual({actual_psk_identity_hint:?})",
        );

        if let Some(result) = client_res_rx.recv().await {
            if let Ok(client) = result {
                client.close().await?;
            } else {
                panic!("{name}: Expected create_test_client successfully, but got error",);
            }
        }

        let _ = server.close().await;
    }

    Ok(())
}

#[tokio::test]
async fn test_psk_hint_fail() -> Result<()> {
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
            psk: Some(Arc::new(psk_callback_hint_fail)),
            psk_identity_hint: Some(vec![]),
            cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8],
            ..Default::default()
        };

        let result = create_test_client(Arc::new(ca), conf, false).await;
        let _ = client_res_tx.send(result).await;
    });

    let config = Config {
        psk: Some(Arc::new(psk_callback_hint_fail)),
        psk_identity_hint: Some(vec![]),
        cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8],
        ..Default::default()
    };

    if let Err(server_err) = create_test_server(Arc::new(cb), config, false).await {
        assert_eq!(
            server_err.to_string(),
            Error::ErrAlertFatalOrClose.to_string(),
            "TestPSK: Server error exp({}) failed({})",
            Error::ErrAlertFatalOrClose,
            server_err,
        );
    } else {
        panic!("Expected server error, but got OK");
    }

    let result = client_res_rx.recv().await;
    if let Some(client) = result {
        if let Err(client_err) = client {
            assert!(
                client_err.to_string().contains(ERR_PSK_REJECTED),
                "TestPSK: Client error exp({ERR_PSK_REJECTED}) failed({client_err})",
            );
        } else {
            panic!("Expected client error, but got OK");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_client_timeout() -> Result<()> {
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
        assert!(client_timeout_result.is_err(), "Expected Error but got Ok");
    }

    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_srtp_configuration() -> Result<()> {
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

    #[allow(clippy::type_complexity)]
    let tests: Vec<(
        &str,
        Vec<SrtpProtectionProfile>,
        Vec<SrtpProtectionProfile>,
        SrtpProtectionProfile,
        Option<Error>,
        Option<Error>,
    )> = vec![
        (
            "No SRTP in use",
            vec![],
            vec![],
            SrtpProtectionProfile::Unsupported,
            None,
            None,
        ),
        (
            "SRTP both ends",
            vec![SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80],
            vec![SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80],
            SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80,
            None,
            None,
        ),
        (
            "SRTP client only",
            vec![SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80],
            vec![],
            SrtpProtectionProfile::Unsupported,
            Some(Error::ErrAlertFatalOrClose),
            Some(Error::ErrServerNoMatchingSrtpProfile),
        ),
        (
            "SRTP server only",
            vec![],
            vec![SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80],
            SrtpProtectionProfile::Unsupported,
            None,
            None,
        ),
        (
            "Multiple Suites",
            vec![
                SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80,
                SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_32,
            ],
            vec![
                SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80,
                SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_32,
            ],
            SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80,
            None,
            None,
        ),
        (
            "Multiple Suites, Client Chooses",
            vec![
                SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80,
                SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_32,
            ],
            vec![
                SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_32,
                SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80,
            ],
            SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80,
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
                    err.to_string(),
                    expected_err.to_string(),
                    "{name} TestPSK: Server error exp({expected_err}) failed({err})",
                );
            } else {
                panic!("{name} expected error, but got ok");
            }
        } else {
            match result {
                Ok(server) => {
                    let actual_server_srtp = server.selected_srtpprotection_profile();
                    assert_eq!(actual_server_srtp, expected_profile,
                               "test_srtp_configuration: Server SRTPProtectionProfile Mismatch '{name}': expected({expected_profile:?}) actual({actual_server_srtp:?})");
                }
                Err(err) => {
                    panic!("{name} expected no error: {err}");
                }
            };
        }

        let client_result = client_res_rx.recv().await;
        if let Some(result) = client_result {
            if let Some(expected_err) = want_client_err {
                if let Err(err) = result {
                    assert_eq!(
                        err.to_string(),
                        expected_err.to_string(),
                        "TestPSK: Client error exp({expected_err}) failed({err})",
                    );
                } else {
                    panic!("{name} expected error, but got ok");
                }
            } else if let Ok(client) = result {
                let actual_client_srtp = client.selected_srtpprotection_profile();
                assert_eq!(actual_client_srtp, expected_profile,
                           "test_srtp_configuration: Client SRTPProtectionProfile Mismatch '{name}': expected({expected_profile:?}) actual({actual_client_srtp:?})");
            } else {
                panic!("{name} expected no error");
            }
        } else {
            panic!("{name} expected client, but got none");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_client_certificate() -> Result<()> {
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
        .add(srv_cert.certificate[0].to_owned())
        .map_err(|_err| Error::Other("add srv_cert error".to_owned()))?;

    let cert = Certificate::generate_self_signed(vec!["localhost".to_owned()])?;
    let mut ca_pool = rustls::RootCertStore::empty();
    ca_pool
        .add(cert.certificate[0].to_owned())
        .map_err(|_err| Error::Other("add cert error".to_owned()))?;

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
                client_cas: ca_pool.clone(),
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
                client_cas: ca_pool.clone(),
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
                client_cas: ca_pool.clone(),
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
            let result = DTLSConn::new(Arc::new(ca), client_cfg_clone, true, None).await;
            let _ = client_res_tx.send(result).await;
        });

        let result = DTLSConn::new(Arc::new(cb), server_cfg.clone(), false, None).await;
        let client_result = client_res_rx.recv().await;

        if want_err {
            if result.is_err() {
                continue;
            }
            panic!("{name} Error expected");
        }

        assert!(
            result.is_ok(),
            "{} Server failed({:?})",
            name,
            result.err().unwrap()
        );
        assert!(client_result.is_some(), "{name}, expected client conn");

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
                "{name} Client did not provide a certificate",
            );
            //if actual_client_cert.len() != len(tt.clientCfg.Certificates[0].Certificate) || !bytes.Equal(tt.clientCfg.Certificates[0].Certificate[0], actual_client_cert[0]) {
            assert_eq!(
                actual_client_cert[0],
                client_cfg.certificates[0].certificate[0].as_ref(),
                "{name} Client certificate was not communicated correctly",
            );
        }

        if server_cfg.client_auth == ClientAuthType::NoClientCert {
            assert!(
                actual_client_cert.is_empty(),
                "{name} Client certificate wasn't expected",
            );
        }

        let actual_server_cert = &client.connection_state().await.peer_certificates;
        assert!(
            !actual_server_cert.is_empty(),
            "{name} Server did not provide a certificate",
        );

        /*if len(actual_server_cert) != len(tt.serverCfg.Certificates[0].Certificate)
        || !bytes.Equal(
            tt.serverCfg.Certificates[0].Certificate[0],
            actual_server_cert[0],
        )*/
        assert_eq!(
            actual_server_cert[0].len(),
            server_cfg.certificates[0].certificate[0].as_ref().len(),
            "{name} Server certificate was not communicated correctly",
        );
        assert_eq!(
            actual_server_cert[0],
            server_cfg.certificates[0].certificate[0].as_ref(),
            "{name} Server certificate was not communicated correctly",
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_extended_master_secret() -> Result<()> {
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
            Some(Error::ErrClientRequiredButNoServerEms),
            Some(Error::ErrAlertFatalOrClose),
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
            Some(Error::ErrAlertFatalOrClose),
            Some(Error::ErrServerRequiredButNoClientEms),
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
        assert!(client_result.is_some(), "{name}, expected client conn");
        let res = client_result.unwrap();

        if let Some(client_err) = expected_client_err {
            if let Err(err) = res {
                assert_eq!(
                    err.to_string(),
                    client_err.to_string(),
                    "Client error expected: \"{client_err}\" but got \"{err}\"",
                );
            } else {
                panic!("{name} expected err, but got ok");
            }
        } else {
            assert!(res.is_ok(), "{name} expected ok, but got err");
        }

        if let Some(server_err) = expected_server_err {
            if let Err(err) = result {
                assert_eq!(
                    err.to_string(),
                    server_err.to_string(),
                    "Server error expected: \"{server_err}\" but got \"{err}\"",
                );
            } else {
                panic!("{name} expected err, but got ok");
            }
        } else {
            assert!(result.is_ok(), "{name} expected ok, but got err");
        }
    }

    Ok(())
}

fn fn_not_expected_chain(_cert: &[Vec<u8>], chain: &[CertificateDer<'static>]) -> Result<()> {
    if !chain.is_empty() {
        return Err(Error::Other(ERR_NOT_EXPECTED_CHAIN.to_owned()));
    }
    Ok(())
}

fn fn_expected_chain(_cert: &[Vec<u8>], chain: &[CertificateDer<'static>]) -> Result<()> {
    if chain.is_empty() {
        return Err(Error::Other(ERR_EXPECTED_CHAIN.to_owned()));
    }
    Ok(())
}

fn fn_wrong_cert(_cert: &[Vec<u8>], _chain: &[CertificateDer<'static>]) -> Result<()> {
    Err(Error::Other(ERR_WRONG_CERT.to_owned()))
}

#[tokio::test]
async fn test_server_certificate() -> Result<()> {
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
    let mut ca_pool = rustls::RootCertStore::empty();
    ca_pool
        .add(cert.certificate[0].clone())
        .map_err(|_err| Error::Other("add cert error".to_owned()))?;

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
                roots_cas: ca_pool.clone(),
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
                roots_cas: ca_pool.clone(),
                server_name: server_name.clone(),
                certificates: vec![cert.clone()],
                ..Default::default()
            },
            Config {
                certificates: vec![cert.clone()],
                client_auth: ClientAuthType::RequireAnyClientCert,
                verify_peer_certificate: Some(Arc::new(fn_not_expected_chain)),
                ..Default::default()
            },
            false,
        ),
        (
            "good_ca_verify_custom_verify_peer",
            Config {
                roots_cas: ca_pool.clone(),
                server_name: server_name.clone(),
                certificates: vec![cert.clone()],
                ..Default::default()
            },
            Config {
                certificates: vec![cert.clone()],
                client_auth: ClientAuthType::RequireAndVerifyClientCert,
                client_cas: ca_pool.clone(),
                verify_peer_certificate: Some(Arc::new(fn_expected_chain)),
                ..Default::default()
            },
            false,
        ),
        (
            "good_ca_custom_verify_peer",
            Config {
                roots_cas: ca_pool.clone(),
                server_name: server_name.clone(),
                verify_peer_certificate: Some(Arc::new(fn_wrong_cert)),
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
                roots_cas: ca_pool.clone(),
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
                roots_cas: ca_pool.clone(),
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
            let result = DTLSConn::new(Arc::new(cb), server_cfg, false, None).await;
            let _ = res_tx.send(result).await;
        });

        let cli_result = DTLSConn::new(Arc::new(ca), client_cfg, true, None).await;

        if !want_err && cli_result.is_err() {
            panic!("{}: Client failed({})", name, cli_result.err().unwrap());
        }
        if want_err && cli_result.is_ok() {
            panic!("{name}: Error expected");
        }

        let _ = res_rx.recv().await;
    }
    Ok(())
}

#[tokio::test]
async fn test_cipher_suite_configuration() -> Result<()> {
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
            vec![CipherSuiteId::Unsupported],
            vec![CipherSuiteId::Unsupported],
            Some(Error::ErrInvalidCipherSuite),
            Some(Error::ErrInvalidCipherSuite),
            None,
        ),
        (
            "Valid CipherSuites specified",
            vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256],
            vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256],
            None,
            None,
            Some(CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256),
        ),
        (
            "CipherSuites mismatch",
            vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256],
            vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_256_Cbc_Sha],
            Some(Error::ErrAlertFatalOrClose),
            Some(Error::ErrCipherSuiteNoIntersection),
            None,
        ),
        (
            "Valid CipherSuites CCM specified",
            vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm],
            vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm],
            None,
            None,
            Some(CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm),
        ),
        (
            "Valid CipherSuites CCM-8 specified",
            vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm_8],
            vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm_8],
            None,
            None,
            Some(CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm_8),
        ),
        (
            "Server supports subset of client suites",
            vec![
                CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256,
                CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_256_Cbc_Sha,
            ],
            vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_256_Cbc_Sha],
            None,
            None,
            Some(CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_256_Cbc_Sha),
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
                    err.to_string(),
                    expected_err.to_string(),
                    "{name} test_cipher_suite_configuration: Server error exp({expected_err}) failed({err})",
                );
            } else {
                panic!("{name} expected error, but got ok");
            }
        } else {
            assert!(result.is_ok(), "{name} expected ok, but got error")
        }

        let client_result = client_res_rx.recv().await;
        if let Some(result) = client_result {
            if let Some(expected_err) = want_client_error {
                if let Err(err) = result {
                    assert_eq!(
                        err.to_string(),
                        expected_err.to_string(),
                        "{name} test_cipher_suite_configuration: Client error exp({expected_err}) failed({err})",
                    );
                } else {
                    panic!("{name} expected error, but got ok");
                }
            } else {
                assert!(result.is_ok(), "{name} expected ok, but got error");
                let client = result.unwrap();
                if let Some(want_cs) = want_selected_cipher_suite {
                    let cipher_suite = client.state.cipher_suite.lock().await;
                    assert!(cipher_suite.is_some(), "{name} expected some, but got none");
                    if let Some(cs) = &*cipher_suite {
                        assert_eq!(cs.id(), want_cs,
                                   "test_cipher_suite_configuration: Server Selected Bad Cipher Suite '{}': expected({}) actual({})", 
                                   name, want_cs, cs.id());
                    }
                }
            }
        } else {
            panic!("{name} expected Some, but got None");
        }
    }

    Ok(())
}

fn psk_callback(_b: &[u8]) -> Result<Vec<u8>> {
    Ok(vec![0x00, 0x01, 0x02])
}

#[tokio::test]
async fn test_psk_configuration() -> Result<()> {
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
            Some(Error::ErrNoAvailableCipherSuites),
            Some(Error::ErrNoAvailableCipherSuites),
        ),
        (
            "PSK and certificate specified",
            true,
            true,
            true, //Some(psk_callback),
            true, //Some(psk_callback),
            Some(vec![0x00]),
            Some(vec![0x00]),
            Some(Error::ErrPskAndCertificate),
            Some(Error::ErrPskAndCertificate),
        ),
        (
            "PSK and no identity specified",
            false,
            false,
            true, //Some(psk_callback),
            true, //Some(psk_callback),
            None,
            None,
            Some(Error::ErrPskAndIdentityMustBeSetForClient),
            Some(Error::ErrNoAvailableCipherSuites),
        ),
        (
            "No PSK and identity specified",
            false,
            false,
            false,
            false,
            Some(vec![0x00]),
            Some(vec![0x00]),
            Some(Error::ErrIdentityNoPsk),
            Some(Error::ErrServerMustHaveCertificate),
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
                psk: if client_psk {
                    Some(Arc::new(psk_callback))
                } else {
                    None
                },
                psk_identity_hint: client_psk_identity,
                ..Default::default()
            };

            let result = create_test_client(Arc::new(ca), conf, client_has_certificate).await;
            let _ = client_res_tx.send(result).await;
        });

        let config = Config {
            psk: if server_psk {
                Some(Arc::new(psk_callback))
            } else {
                None
            },
            psk_identity_hint: server_psk_identity,
            ..Default::default()
        };

        let result = create_test_server(Arc::new(cb), config, server_has_certificate).await;
        if let Some(expected_err) = want_server_error {
            if let Err(err) = result {
                assert_eq!(
                    err.to_string(),
                    expected_err.to_string(),
                    "{name} test_psk_configuration: Server error exp({expected_err}) failed({err})",
                );
            } else {
                panic!("{name} expected error, but got ok");
            }
        } else {
            assert!(result.is_ok(), "{name} expected ok, but got error")
        }

        let client_result = client_res_rx.recv().await;
        if let Some(result) = client_result {
            if let Some(expected_err) = want_client_error {
                if let Err(err) = result {
                    assert_eq!(
                        err.to_string(),
                        expected_err.to_string(),
                        "{name} test_psk_configuration: Client error exp({expected_err}) failed({err})",
                    );
                } else {
                    panic!("{name} expected error, but got ok");
                }
            } else {
                assert!(result.is_ok(), "{name} expected ok, but got error");
            }
        } else {
            panic!("{name} expected Some, but got None");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_server_timeout() -> Result<()> {
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
        CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256, //&cipherSuiteTLSEcdheEcdsaWithAes128GcmSha256{},
        CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256, //&cipherSuiteTLSEcdheRsaWithAes128GcmSha256{},
    ];

    let extensions = vec![
        Extension::SupportedSignatureAlgorithms(ExtensionSupportedSignatureAlgorithms {
            signature_hash_algorithms: vec![
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::Sha256,
                    signature: SignatureAlgorithm::Ecdsa,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::Sha384,
                    signature: SignatureAlgorithm::Ecdsa,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::Sha512,
                    signature: SignatureAlgorithm::Ecdsa,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::Sha256,
                    signature: SignatureAlgorithm::Rsa,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::Sha384,
                    signature: SignatureAlgorithm::Rsa,
                },
                SignatureHashAlgorithm {
                    hash: HashAlgorithm::Sha512,
                    signature: SignatureAlgorithm::Rsa,
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
        cipher_suites: vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256],
        flight_interval: Duration::from_millis(100),
        ..Default::default()
    };

    let result = tokio::time::timeout(
        Duration::from_millis(50),
        create_test_server(Arc::new(cb), config, true),
    )
    .await;
    assert!(result.is_err(), "Expected Error but got Ok");

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
async fn test_protocol_version_validation() -> Result<()> {
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
                                CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256,
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
                                    CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256,
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
                                    CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256,
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
                    cipher_suites: vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256],
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
                                err.to_string(),
                                Error::ErrUnsupportedProtocolVersion.to_string(),
                                "{} Client error exp({}) failed({})",
                                name,
                                Error::ErrUnsupportedProtocolVersion,
                                err,
                            );
                        } else {
                            panic!("{name} expected error, but got ok");
                        }
                    }
                    Err(err) => {
                        panic!("server timeout {err}");
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
                            cipher_suite: CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256,
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
                            algorithm: SignatureHashAlgorithm {
                                hash: HashAlgorithm::Sha256,
                                signature: SignatureAlgorithm::Ecdsa,
                            },
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
                    cipher_suites: vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256],
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
                                err.to_string(),
                                Error::ErrUnsupportedProtocolVersion.to_string(),
                                "{} Server error exp({}) failed({})",
                                name,
                                Error::ErrUnsupportedProtocolVersion,
                                err,
                            );
                        } else {
                            panic!("{name} expected error, but got ok");
                        }
                    }
                    Err(err) => {
                        panic!("server timeout {err}");
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
async fn test_multiple_hello_verify_request() -> Result<()> {
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
                _ => panic!("unexpected handshake message"),
            },
            _ => panic!("unexpected content"),
        };

        if packets.len() <= i {
            break;
        }
        // write hello verify request
        cb.send(&packets[i]).await?;
    }

    Ok(())
}

async fn send_client_hello(
    cookie: Vec<u8>,
    ca: &Arc<dyn Conn + Send + Sync>,
    sequence_number: u64,
    send_renegotiation_info: bool,
) -> Result<()> {
    let mut extensions = vec![];
    if send_renegotiation_info {
        extensions.push(Extension::RenegotiationInfo(ExtensionRenegotiationInfo {
            renegotiated_connection: 0,
        }));
    }

    let mut h = Handshake::new(HandshakeMessage::ClientHello(HandshakeMessageClientHello {
        version: PROTOCOL_VERSION1_2,
        random: HandshakeRandom::default(),
        cookie,

        cipher_suites: vec![CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256],
        compression_methods: default_compression_methods(),
        extensions,
    }));
    h.handshake_header.message_sequence = sequence_number as u16;

    let mut record = RecordLayer::new(PROTOCOL_VERSION1_2, 0, Content::Handshake(h));
    record.record_layer_header.sequence_number = sequence_number;

    let mut packet = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(packet.as_mut());
        record.marshal(&mut writer)?;
    }

    ca.send(&packet).await?;

    Ok(())
}

// Assert that a DTLS Server always responds with RenegotiationInfo if
// a ClientHello contained that extension or not
#[cfg(not(target_os = "windows"))] // this times out in CI on windows.
#[tokio::test]
async fn test_renegotiation_info() -> Result<()> {
    let mut resp = vec![0u8; 1024];

    let tests = vec![
        ("Include RenegotiationInfo", true),
        ("No RenegotiationInfo", false),
    ];

    for (name, send_renegotiation_info) in tests {
        let (ca, cb) = pipe();

        tokio::spawn(async move {
            let conf = Config::default();
            let _ = tokio::time::timeout(
                Duration::from_millis(100),
                create_test_server(Arc::new(cb), conf, true),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(5)).await;

        let ca: Arc<dyn Conn + Send + Sync> = Arc::new(ca);
        send_client_hello(vec![], &ca, 0, send_renegotiation_info).await?;

        let n = ca.recv(&mut resp).await?;
        let mut reader = BufReader::new(&resp[..n]);
        let record = RecordLayer::unmarshal(&mut reader)?;

        let hello_verify_request = match record.content {
            Content::Handshake(h) => match h.handshake_message {
                HandshakeMessage::HelloVerifyRequest(hvr) => hvr,
                _ => {
                    panic!("unexpected handshake message");
                }
            },
            _ => {
                panic!("unexpected content");
            }
        };

        send_client_hello(
            hello_verify_request.cookie.clone(),
            &ca,
            1,
            send_renegotiation_info,
        )
        .await?;
        let n = ca.recv(&mut resp).await?;
        let messages = unpack_datagram(&resp[..n])?;

        let mut reader = BufReader::new(&messages[0][..]);
        let record = RecordLayer::unmarshal(&mut reader)?;

        let server_hello = match record.content {
            Content::Handshake(h) => match h.handshake_message {
                HandshakeMessage::ServerHello(sh) => sh,
                _ => {
                    panic!("unexpected handshake message");
                }
            },
            _ => {
                panic!("unexpected content");
            }
        };

        let got_negotiation_info = server_hello
            .extensions
            .iter()
            .any(|v| matches!(v, Extension::RenegotiationInfo(_)));

        assert!(
            got_negotiation_info,
            "{name}: Received ServerHello without RenegotiationInfo"
        );

        ca.close().await?;
    }

    Ok(())
}
