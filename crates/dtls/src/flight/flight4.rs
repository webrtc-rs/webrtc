use super::flight6::*;
use super::*;
use crate::cipher_suite::*;
use crate::client_certificate_type::*;
use crate::compression_methods::*;
use crate::config::*;
use crate::content::*;
use crate::crypto::*;
use crate::curve::named_curve::*;
use crate::curve::*;
use crate::error::Error;
use crate::extension::extension_supported_elliptic_curves::*;
use crate::extension::extension_supported_point_formats::*;
use crate::extension::extension_use_extended_master_secret::*;
use crate::extension::extension_use_srtp::*;
use crate::extension::*;
use crate::handshake::handshake_message_certificate::*;
use crate::handshake::handshake_message_certificate_request::*;
use crate::handshake::handshake_message_server_hello::*;
use crate::handshake::handshake_message_server_hello_done::*;
use crate::handshake::handshake_message_server_key_exchange::*;
use crate::handshake::*;
use crate::prf::*;
use crate::record_layer::record_layer_header::*;
use crate::record_layer::*;
use crate::signature_hash_algorithm::*;

use crate::extension::renegotiation_info::ExtensionRenegotiationInfo;
use async_trait::async_trait;
use log::*;
use std::fmt;
use std::io::BufWriter;

#[derive(Debug, PartialEq)]
pub(crate) struct Flight4;

impl fmt::Display for Flight4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Flight 4")
    }
}

#[async_trait]
impl Flight for Flight4 {
    async fn parse(
        &self,
        tx: &mut mpsc::Sender<mpsc::Sender<()>>,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Box<dyn Flight + Send + Sync>, (Option<Alert>, Option<Error>)> {
        let (seq, msgs) = match cache
            .full_pull_map(
                state.handshake_recv_sequence,
                &[
                    HandshakeCachePullRule {
                        typ: HandshakeType::Certificate,
                        epoch: cfg.initial_epoch,
                        is_client: true,
                        optional: true,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::ClientKeyExchange,
                        epoch: cfg.initial_epoch,
                        is_client: true,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::CertificateVerify,
                        epoch: cfg.initial_epoch,
                        is_client: true,
                        optional: true,
                    },
                ],
            )
            .await
        {
            Ok((seq, msgs)) => (seq, msgs),
            Err(_) => return Err((None, None)),
        };

        let client_key_exchange = if let Some(HandshakeMessage::ClientKeyExchange(h)) =
            msgs.get(&HandshakeType::ClientKeyExchange)
        {
            h
        } else {
            return Err((
                Some(Alert {
                    alert_level: AlertLevel::Fatal,
                    alert_description: AlertDescription::InternalError,
                }),
                None,
            ));
        };

        if let Some(message) = msgs.get(&HandshakeType::Certificate) {
            let h = match message {
                HandshakeMessage::Certificate(h) => h,
                _ => {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::InternalError,
                        }),
                        None,
                    ))
                }
            };

            state.peer_certificates = h.certificate.clone();
            trace!(
                "[handshake] PeerCertificates4 {}",
                state.peer_certificates.len()
            );
        }

        if let Some(message) = msgs.get(&HandshakeType::CertificateVerify) {
            let h = match message {
                HandshakeMessage::CertificateVerify(h) => h,
                _ => {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::InternalError,
                        }),
                        None,
                    ))
                }
            };

            if state.peer_certificates.is_empty() {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::NoCertificate,
                    }),
                    Some(Error::ErrCertificateVerifyNoCertificate),
                ));
            }

            let plain_text = cache
                .pull_and_merge(&[
                    HandshakeCachePullRule {
                        typ: HandshakeType::ClientHello,
                        epoch: cfg.initial_epoch,
                        is_client: true,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::ServerHello,
                        epoch: cfg.initial_epoch,
                        is_client: false,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::Certificate,
                        epoch: cfg.initial_epoch,
                        is_client: false,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::ServerKeyExchange,
                        epoch: cfg.initial_epoch,
                        is_client: false,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::CertificateRequest,
                        epoch: cfg.initial_epoch,
                        is_client: false,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::ServerHelloDone,
                        epoch: cfg.initial_epoch,
                        is_client: false,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::Certificate,
                        epoch: cfg.initial_epoch,
                        is_client: true,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::ClientKeyExchange,
                        epoch: cfg.initial_epoch,
                        is_client: true,
                        optional: false,
                    },
                ])
                .await;

            // Verify that the pair of hash algorithm and signature is listed.
            let mut valid_signature_scheme = false;
            for ss in &cfg.local_signature_schemes {
                if ss.hash == h.algorithm.hash && ss.signature == h.algorithm.signature {
                    valid_signature_scheme = true;
                    break;
                }
            }
            if !valid_signature_scheme {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::InsufficientSecurity,
                    }),
                    Some(Error::ErrNoAvailableSignatureSchemes),
                ));
            }

            if let Err(err) = verify_certificate_verify(
                &plain_text,
                &h.algorithm,
                &h.signature,
                &state.peer_certificates,
            ) {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::BadCertificate,
                    }),
                    Some(err),
                ));
            }

            let mut chains = vec![];
            let mut verified = false;
            if cfg.client_auth as u8 >= ClientAuthType::VerifyClientCertIfGiven as u8 {
                if let Some(client_cert_verifier) = &cfg.client_cert_verifier {
                    chains =
                        match verify_client_cert(&state.peer_certificates, client_cert_verifier) {
                            Ok(chains) => chains,
                            Err(err) => {
                                return Err((
                                    Some(Alert {
                                        alert_level: AlertLevel::Fatal,
                                        alert_description: AlertDescription::BadCertificate,
                                    }),
                                    Some(err),
                                ))
                            }
                        };
                } else {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::BadCertificate,
                        }),
                        Some(Error::ErrInvalidCertificate),
                    ));
                }

                verified = true
            }
            if let Some(verify_peer_certificate) = &cfg.verify_peer_certificate {
                if let Err(err) = verify_peer_certificate(&state.peer_certificates, &chains) {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::BadCertificate,
                        }),
                        Some(err),
                    ));
                }
            }
            state.peer_certificates_verified = verified
        }

        {
            let mut cipher_suite = state.cipher_suite.lock().await;
            if let Some(cipher_suite) = &mut *cipher_suite {
                if !cipher_suite.is_initialized() {
                    let mut server_random = vec![];
                    {
                        let mut writer = BufWriter::<&mut Vec<u8>>::new(server_random.as_mut());
                        let _ = state.local_random.marshal(&mut writer);
                    }
                    let mut client_random = vec![];
                    {
                        let mut writer = BufWriter::<&mut Vec<u8>>::new(client_random.as_mut());
                        let _ = state.remote_random.marshal(&mut writer);
                    }

                    let mut pre_master_secret = vec![];
                    if let Some(local_psk_callback) = &cfg.local_psk_callback {
                        let psk = match local_psk_callback(&client_key_exchange.identity_hint) {
                            Ok(psk) => psk,
                            Err(err) => {
                                return Err((
                                    Some(Alert {
                                        alert_level: AlertLevel::Fatal,
                                        alert_description: AlertDescription::InternalError,
                                    }),
                                    Some(err),
                                ))
                            }
                        };

                        state.identity_hint = client_key_exchange.identity_hint.clone();
                        pre_master_secret = prf_psk_pre_master_secret(&psk);
                    } else if let Some(local_keypair) = &state.local_keypair {
                        pre_master_secret = match prf_pre_master_secret(
                            &client_key_exchange.public_key,
                            &local_keypair.private_key,
                            local_keypair.curve,
                        ) {
                            Ok(pre_master_secret) => pre_master_secret,
                            Err(err) => {
                                return Err((
                                    Some(Alert {
                                        alert_level: AlertLevel::Fatal,
                                        alert_description: AlertDescription::IllegalParameter,
                                    }),
                                    Some(err),
                                ))
                            }
                        };
                    }

                    if state.extended_master_secret {
                        let hf = cipher_suite.hash_func();
                        let session_hash =
                            match cache.session_hash(hf, cfg.initial_epoch, &[]).await {
                                Ok(s) => s,
                                Err(err) => {
                                    return Err((
                                        Some(Alert {
                                            alert_level: AlertLevel::Fatal,
                                            alert_description: AlertDescription::InternalError,
                                        }),
                                        Some(err),
                                    ))
                                }
                            };

                        state.master_secret = match prf_extended_master_secret(
                            &pre_master_secret,
                            &session_hash,
                            cipher_suite.hash_func(),
                        ) {
                            Ok(ms) => ms,
                            Err(err) => {
                                return Err((
                                    Some(Alert {
                                        alert_level: AlertLevel::Fatal,
                                        alert_description: AlertDescription::InternalError,
                                    }),
                                    Some(err),
                                ))
                            }
                        };
                    } else {
                        state.master_secret = match prf_master_secret(
                            &pre_master_secret,
                            &client_random,
                            &server_random,
                            cipher_suite.hash_func(),
                        ) {
                            Ok(ms) => ms,
                            Err(err) => {
                                return Err((
                                    Some(Alert {
                                        alert_level: AlertLevel::Fatal,
                                        alert_description: AlertDescription::InternalError,
                                    }),
                                    Some(err),
                                ))
                            }
                        };
                    }

                    if let Err(err) = cipher_suite.init(
                        &state.master_secret,
                        &client_random,
                        &server_random,
                        false,
                    ) {
                        return Err((
                            Some(Alert {
                                alert_level: AlertLevel::Fatal,
                                alert_description: AlertDescription::InternalError,
                            }),
                            Some(err),
                        ));
                    }
                }
            }
        }

        // Now, encrypted packets can be handled
        let (done_tx, mut done_rx) = mpsc::channel(1);
        if let Err(err) = tx.send(done_tx).await {
            return Err((
                Some(Alert {
                    alert_level: AlertLevel::Fatal,
                    alert_description: AlertDescription::InternalError,
                }),
                Some(Error::Other(err.to_string())),
            ));
        }

        done_rx.recv().await;

        let (seq, msgs) = match cache
            .full_pull_map(
                seq,
                &[HandshakeCachePullRule {
                    typ: HandshakeType::Finished,
                    epoch: cfg.initial_epoch + 1,
                    is_client: true,
                    optional: false,
                }],
            )
            .await
        {
            Ok((seq, msgs)) => (seq, msgs),
            // No valid message received. Keep reading
            Err(_) => return Err((None, None)),
        };

        state.handshake_recv_sequence = seq;

        if let Some(HandshakeMessage::Finished(h)) = msgs.get(&HandshakeType::Finished) {
            h
        } else {
            return Err((
                Some(Alert {
                    alert_level: AlertLevel::Fatal,
                    alert_description: AlertDescription::InternalError,
                }),
                None,
            ));
        };

        match cfg.client_auth {
            ClientAuthType::RequireAnyClientCert => {
                trace!(
                    "{} peer_certificates.len() {}",
                    srv_cli_str(state.is_client),
                    state.peer_certificates.len(),
                );
                if state.peer_certificates.is_empty() {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::NoCertificate,
                        }),
                        Some(Error::ErrClientCertificateRequired),
                    ));
                }
            }
            ClientAuthType::VerifyClientCertIfGiven => {
                if !state.peer_certificates.is_empty() && !state.peer_certificates_verified {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::BadCertificate,
                        }),
                        Some(Error::ErrClientCertificateNotVerified),
                    ));
                }
            }
            ClientAuthType::RequireAndVerifyClientCert => {
                if state.peer_certificates.is_empty() {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::NoCertificate,
                        }),
                        Some(Error::ErrClientCertificateRequired),
                    ));
                }
                if !state.peer_certificates_verified {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::BadCertificate,
                        }),
                        Some(Error::ErrClientCertificateNotVerified),
                    ));
                }
            }
            ClientAuthType::NoClientCert | ClientAuthType::RequestClientCert => {
                return Ok(Box::new(Flight6 {}) as Box<dyn Flight + Send + Sync>);
            }
        }

        Ok(Box::new(Flight6 {}) as Box<dyn Flight + Send + Sync>)
    }

    async fn generate(
        &self,
        state: &mut State,
        _cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Vec<Packet>, (Option<Alert>, Option<Error>)> {
        let mut extensions = vec![Extension::RenegotiationInfo(ExtensionRenegotiationInfo {
            renegotiated_connection: 0,
        })];
        if (cfg.extended_master_secret == ExtendedMasterSecretType::Request
            || cfg.extended_master_secret == ExtendedMasterSecretType::Require)
            && state.extended_master_secret
        {
            extensions.push(Extension::UseExtendedMasterSecret(
                ExtensionUseExtendedMasterSecret { supported: true },
            ));
        }

        if state.srtp_protection_profile != SrtpProtectionProfile::Unsupported {
            extensions.push(Extension::UseSrtp(ExtensionUseSrtp {
                protection_profiles: vec![state.srtp_protection_profile],
            }));
        }

        if cfg.local_psk_callback.is_none() {
            extensions.extend_from_slice(&[
                Extension::SupportedEllipticCurves(ExtensionSupportedEllipticCurves {
                    elliptic_curves: vec![NamedCurve::P256, NamedCurve::X25519, NamedCurve::P384],
                }),
                Extension::SupportedPointFormats(ExtensionSupportedPointFormats {
                    point_formats: vec![ELLIPTIC_CURVE_POINT_FORMAT_UNCOMPRESSED],
                }),
            ]);
        }

        let mut pkts = vec![Packet {
            record: RecordLayer::new(
                PROTOCOL_VERSION1_2,
                0,
                Content::Handshake(Handshake::new(HandshakeMessage::ServerHello(
                    HandshakeMessageServerHello {
                        version: PROTOCOL_VERSION1_2,
                        random: state.local_random.clone(),
                        cipher_suite: {
                            let cipher_suite = state.cipher_suite.lock().await;
                            if let Some(cipher_suite) = &*cipher_suite {
                                cipher_suite.id()
                            } else {
                                CipherSuiteId::Unsupported
                            }
                        },
                        compression_method: default_compression_methods().ids[0],
                        extensions,
                    },
                ))),
            ),
            should_encrypt: false,
            reset_local_sequence_number: false,
        }];

        if cfg.local_psk_callback.is_none() {
            let certificate = match cfg.get_certificate(&cfg.server_name) {
                Ok(cert) => cert,
                Err(err) => {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::HandshakeFailure,
                        }),
                        Some(err),
                    ))
                }
            };

            pkts.push(Packet {
                record: RecordLayer::new(
                    PROTOCOL_VERSION1_2,
                    0,
                    Content::Handshake(Handshake::new(HandshakeMessage::Certificate(
                        HandshakeMessageCertificate {
                            certificate: certificate
                                .certificate
                                .iter()
                                .map(|x| x.0.clone())
                                .collect(),
                        },
                    ))),
                ),
                should_encrypt: false,
                reset_local_sequence_number: false,
            });

            let mut server_random = vec![];
            {
                let mut writer = BufWriter::<&mut Vec<u8>>::new(server_random.as_mut());
                let _ = state.local_random.marshal(&mut writer);
            }
            let mut client_random = vec![];
            {
                let mut writer = BufWriter::<&mut Vec<u8>>::new(client_random.as_mut());
                let _ = state.remote_random.marshal(&mut writer);
            }

            // Find compatible signature scheme
            let signature_hash_algo = match select_signature_scheme(
                &cfg.local_signature_schemes,
                &certificate.private_key,
            ) {
                Ok(s) => s,
                Err(err) => {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::InsufficientSecurity,
                        }),
                        Some(err),
                    ))
                }
            };

            if let Some(local_keypair) = &state.local_keypair {
                let signature = match generate_key_signature(
                    &client_random,
                    &server_random,
                    &local_keypair.public_key,
                    state.named_curve,
                    &certificate.private_key, /*, signature_hash_algo.hash*/
                ) {
                    Ok(s) => s,
                    Err(err) => {
                        return Err((
                            Some(Alert {
                                alert_level: AlertLevel::Fatal,
                                alert_description: AlertDescription::InternalError,
                            }),
                            Some(err),
                        ))
                    }
                };

                state.local_key_signature = signature;

                pkts.push(Packet {
                    record: RecordLayer::new(
                        PROTOCOL_VERSION1_2,
                        0,
                        Content::Handshake(Handshake::new(HandshakeMessage::ServerKeyExchange(
                            HandshakeMessageServerKeyExchange {
                                identity_hint: vec![],
                                elliptic_curve_type: EllipticCurveType::NamedCurve,
                                named_curve: state.named_curve,
                                public_key: local_keypair.public_key.clone(),
                                algorithm: SignatureHashAlgorithm {
                                    hash: signature_hash_algo.hash,
                                    signature: signature_hash_algo.signature,
                                },
                                signature: state.local_key_signature.clone(),
                            },
                        ))),
                    ),
                    should_encrypt: false,
                    reset_local_sequence_number: false,
                });
            }

            if cfg.client_auth as u8 > ClientAuthType::NoClientCert as u8 {
                pkts.push(Packet {
                    record: RecordLayer::new(
                        PROTOCOL_VERSION1_2,
                        0,
                        Content::Handshake(Handshake::new(HandshakeMessage::CertificateRequest(
                            HandshakeMessageCertificateRequest {
                                certificate_types: vec![
                                    ClientCertificateType::RsaSign,
                                    ClientCertificateType::EcdsaSign,
                                ],
                                signature_hash_algorithms: cfg.local_signature_schemes.clone(),
                            },
                        ))),
                    ),
                    should_encrypt: false,
                    reset_local_sequence_number: false,
                });
            }
        } else if let Some(local_psk_identity_hint) = &cfg.local_psk_identity_hint {
            // To help the client in selecting which identity to use, the server
            // can provide a "PSK identity hint" in the ServerKeyExchange message.
            // If no hint is provided, the ServerKeyExchange message is omitted.
            //
            // https://tools.ietf.org/html/rfc4279#section-2
            pkts.push(Packet {
                record: RecordLayer::new(
                    PROTOCOL_VERSION1_2,
                    0,
                    Content::Handshake(Handshake::new(HandshakeMessage::ServerKeyExchange(
                        HandshakeMessageServerKeyExchange {
                            identity_hint: local_psk_identity_hint.clone(),
                            elliptic_curve_type: EllipticCurveType::Unsupported,
                            named_curve: NamedCurve::Unsupported,
                            public_key: vec![],
                            algorithm: SignatureHashAlgorithm {
                                hash: HashAlgorithm::Unsupported,
                                signature: SignatureAlgorithm::Unsupported,
                            },
                            signature: vec![],
                        },
                    ))),
                ),
                should_encrypt: false,
                reset_local_sequence_number: false,
            });
        }

        pkts.push(Packet {
            record: RecordLayer::new(
                PROTOCOL_VERSION1_2,
                0,
                Content::Handshake(Handshake::new(HandshakeMessage::ServerHelloDone(
                    HandshakeMessageServerHelloDone {},
                ))),
            ),
            should_encrypt: false,
            reset_local_sequence_number: false,
        });

        Ok(pkts)
    }
}
