use std::fmt;

use async_trait::async_trait;

use super::*;
use crate::change_cipher_spec::*;
use crate::content::*;
use crate::handshake::handshake_message_finished::*;
use crate::handshake::*;
use crate::prf::*;
use crate::record_layer::record_layer_header::*;

#[derive(Debug, PartialEq)]
pub(crate) struct Flight6;

impl fmt::Display for Flight6 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Flight 6")
    }
}

#[async_trait]
impl Flight for Flight6 {
    fn is_last_send_flight(&self) -> bool {
        true
    }

    async fn parse(
        &self,
        _tx: &mut mpsc::Sender<mpsc::Sender<()>>,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Box<dyn Flight + Send + Sync>, (Option<Alert>, Option<Error>)> {
        let (_, msgs) = match cache
            .full_pull_map(
                state.handshake_recv_sequence - 1,
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

        if let Some(message) = msgs.get(&HandshakeType::Finished) {
            match message {
                HandshakeMessage::Finished(_) => {}
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
        }

        // Other party retransmitted the last flight.
        Ok(Box::new(Flight6 {}))
    }

    async fn generate(
        &self,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Vec<Packet>, (Option<Alert>, Option<Error>)> {
        let mut pkts = vec![Packet {
            record: RecordLayer::new(
                PROTOCOL_VERSION1_2,
                0,
                Content::ChangeCipherSpec(ChangeCipherSpec {}),
            ),
            should_encrypt: false,
            reset_local_sequence_number: false,
        }];

        if state.local_verify_data.is_empty() {
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
                    HandshakeCachePullRule {
                        typ: HandshakeType::CertificateVerify,
                        epoch: cfg.initial_epoch,
                        is_client: true,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::Finished,
                        epoch: cfg.initial_epoch + 1,
                        is_client: true,
                        optional: false,
                    },
                ])
                .await;

            let cipher_suite = state.cipher_suite.lock().await;
            if let Some(cipher_suite) = &*cipher_suite {
                state.local_verify_data = match prf_verify_data_server(
                    &state.master_secret,
                    &plain_text,
                    cipher_suite.hash_func(),
                ) {
                    Ok(data) => data,
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
        }

        pkts.push(Packet {
            record: RecordLayer::new(
                PROTOCOL_VERSION1_2,
                1,
                Content::Handshake(Handshake::new(HandshakeMessage::Finished(
                    HandshakeMessageFinished {
                        verify_data: state.local_verify_data.clone(),
                    },
                ))),
            ),
            should_encrypt: true,
            reset_local_sequence_number: true,
        });

        Ok(pkts)
    }
}
