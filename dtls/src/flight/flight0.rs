use super::*;
use crate::config::*;
use crate::errors::*;
use crate::extension::*;
use crate::handshake::*;
use crate::record_layer::record_layer_header::*;
use crate::*;

use util::Error;

pub(crate) async fn flight0parse<C: FlightConn>(
    /*context.Context,*/
    _c: C,
    state: &mut State,
    cache: &HandshakeCache,
    cfg: &HandshakeConfig,
) -> Result<Flight, (Option<Alert>, Option<Error>)> {
    let (seq, msgs) = match cache
        .full_pull_map(
            0,
            &[HandshakeCachePullRule {
                typ: HandshakeType::ClientHello,
                epoch: cfg.initial_epoch,
                is_client: true,
                optional: false,
            }],
        )
        .await
    {
        Ok((seq, msgs)) => (seq, msgs),
        Err(_) => return Err((None, None)),
    };

    state.handshake_recv_sequence = seq;

    if let Some(message) = msgs.get(&HandshakeType::ClientHello) {
        // Validate type
        let client_hello = match message {
            HandshakeMessage::ClientHello(client_hello) => client_hello,
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

        if client_hello.version != PROTOCOL_VERSION1_2 {
            return Err((
                Some(Alert {
                    alert_level: AlertLevel::Fatal,
                    alert_description: AlertDescription::ProtocolVersion,
                }),
                Some(ERR_UNSUPPORTED_PROTOCOL_VERSION.clone()),
            ));
        }

        state.remote_random = client_hello.random.clone();

        if let Ok(id) =
            find_matching_cipher_suite(&client_hello.cipher_suites, &cfg.local_cipher_suites)
        {
            if let Ok(cs) = cipher_suite_for_id(id) {
                state.cipher_suite = Some(cs);
            }
        } else {
            return Err((
                Some(Alert {
                    alert_level: AlertLevel::Fatal,
                    alert_description: AlertDescription::InsufficientSecurity,
                }),
                Some(ERR_CIPHER_SUITE_NO_INTERSECTION.clone()),
            ));
        }

        for extension in &client_hello.extensions {
            match extension {
                Extension::SupportedEllipticCurves(e) => {
                    if e.elliptic_curves.is_empty() {
                        return Err((
                            Some(Alert {
                                alert_level: AlertLevel::Fatal,
                                alert_description: AlertDescription::InsufficientSecurity,
                            }),
                            Some(ERR_NO_SUPPORTED_ELLIPTIC_CURVES.clone()),
                        ));
                    }
                    state.named_curve = e.elliptic_curves[0];
                }
                Extension::UseSRTP(e) => {
                    if let Ok(profile) = find_matching_srtp_profile(
                        &e.protection_profiles,
                        &cfg.local_srtp_protection_profiles,
                    ) {
                        state.srtp_protection_profile = profile;
                    } else {
                        return Err((
                            Some(Alert {
                                alert_level: AlertLevel::Fatal,
                                alert_description: AlertDescription::InsufficientSecurity,
                            }),
                            Some(ERR_SERVER_NO_MATCHING_SRTP_PROFILE.clone()),
                        ));
                    }
                }
                Extension::UseExtendedMasterSecret(_) => {
                    if cfg.extended_master_secret != ExtendedMasterSecretType::Disable {
                        state.extended_master_secret = true;
                    }
                }
                Extension::ServerName(e) => {
                    state.server_name = e.server_name.clone(); // remote server name
                }
                _ => {}
            }
        }

        if cfg.extended_master_secret == ExtendedMasterSecretType::Require
            && !state.extended_master_secret
        {
            return Err((
                Some(Alert {
                    alert_level: AlertLevel::Fatal,
                    alert_description: AlertDescription::InsufficientSecurity,
                }),
                Some(ERR_SERVER_REQUIRED_BUT_NO_CLIENT_EMS.clone()),
            ));
        }

        if state.local_keypair.is_none() {
            state.local_keypair = match state.named_curve.generate_keypair() {
                Ok(local_keypar) => Some(local_keypar),
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

        Ok(Flight::Flight2)
    } else {
        Err((
            Some(Alert {
                alert_level: AlertLevel::Fatal,
                alert_description: AlertDescription::InternalError,
            }),
            None,
        ))
    }
}
