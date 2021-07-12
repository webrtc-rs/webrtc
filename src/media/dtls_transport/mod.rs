pub mod dtls_fingerprint;
pub mod dtls_parameters;
pub mod dtls_role;
pub mod dtls_transport_state;

use dtls_role::*;

use crate::api::setting_engine::SettingEngine;
use crate::default_srtp_protection_profiles;
use crate::error::Error;
use crate::media::dtls_transport::dtls_transport_state::DTLSTransportState;
use crate::media::ice_transport::ice_transport_state::ICETransportState;
use crate::media::ice_transport::ICETransport;
use crate::peer::ice::ice_role::ICERole;
use crate::util::mux::endpoint::Endpoint;
use crate::util::mux::mux_func::{match_dtls, match_srtcp, match_srtp, MatchFunc};
use bytes::Bytes;
use dtls::config::ClientAuthType;
use dtls::conn::DTLSConn;
use dtls::crypto::Certificate;
use srtp::protection_profile::ProtectionProfile;
use srtp::session::Session;
use srtp::stream::Stream;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use util::Conn;

use crate::media::dtls_transport::dtls_parameters::DTLSParameters;
use anyhow::Result;

pub type OnStateChangeHdlrFn = Box<
    dyn (FnMut(DTLSTransportState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

/// DTLSTransport allows an application access to information about the DTLS
/// transport over which RTP and RTCP packets are sent and received by
/// RTPSender and RTPReceiver, as well other data such as SCTP packets sent
/// and received by data channels.
#[derive(Default)]
pub struct DTLSTransport {
    pub(crate) ice_transport: ICETransport,
    pub(crate) certificates: Vec<Certificate>,
    pub(crate) setting_engine: SettingEngine,

    pub(crate) remote_parameters: DTLSParameters,
    pub(crate) remote_certificate: Bytes,
    pub(crate) state: DTLSTransportState,
    pub(crate) srtp_protection_profile: ProtectionProfile,
    pub(crate) on_state_change_handler: Arc<Mutex<Option<OnStateChangeHdlrFn>>>,
    pub(crate) conn: Option<Arc<DTLSConn>>,

    pub(crate) srtp_session: Option<Session>,
    pub(crate) srtcp_session: Option<Session>,
    pub(crate) srtp_endpoint: Option<Arc<Endpoint>>,
    pub(crate) srtcp_endpoint: Option<Arc<Endpoint>>,

    pub(crate) simulcast_streams: Vec<Stream>,
    pub(crate) srtp_ready_tx: Option<mpsc::Sender<()>>,
    pub(crate) srtp_ready_rx: Option<mpsc::Receiver<()>>,

    pub(crate) dtls_matcher: Option<MatchFunc>,
}

impl DTLSTransport {
    pub fn new(
        ice_transport: ICETransport,
        certificates: Vec<Certificate>,
        setting_engine: SettingEngine,
    ) -> Self {
        let (srtp_ready_tx, srtp_ready_rx) = mpsc::channel(1);
        DTLSTransport {
            ice_transport,
            certificates,
            setting_engine,
            srtp_ready_tx: Some(srtp_ready_tx),
            srtp_ready_rx: Some(srtp_ready_rx),
            state: DTLSTransportState::New,
            dtls_matcher: Some(Box::new(match_dtls)),
            ..Default::default()
        }
    }

    /// returns the currently-configured ICETransport or None
    /// if one has not been configured
    pub fn ice_transport(&self) -> &ICETransport {
        &self.ice_transport
    }

    /// state_change requires the caller holds the lock
    async fn state_change(&mut self, state: DTLSTransportState) {
        self.state = state;
        let mut handler = self.on_state_change_handler.lock().await;
        if let Some(f) = &mut *handler {
            f(state).await;
        }
    }

    /// on_state_change sets a handler that is fired when the DTLS
    /// connection state changes.
    pub async fn on_state_change(&self, f: OnStateChangeHdlrFn) {
        let mut on_state_change_handler = self.on_state_change_handler.lock().await;
        *on_state_change_handler = Some(f);
    }

    /// state returns the current dtls_transport transport state.
    pub fn state(&self) -> DTLSTransportState {
        self.state
    }

    /// write_rtcp sends a user provided RTCP packet to the connected peer. If no peer is connected the
    /// packet is discarded.
    pub async fn write_rtcp(&mut self, pkt: &(dyn rtcp::packet::Packet)) -> Result<usize> {
        if let Some(srtcp_session) = &mut self.srtcp_session {
            Ok(srtcp_session.write_rtcp(pkt).await?)
        } else {
            Ok(0)
        }
    }

    /// get_local_parameters returns the DTLS parameters of the local DTLSTransport upon construction.
    pub fn get_local_parameters(&self) -> Result<DTLSParameters> {
        let fingerprints = vec![];

        for _c in &self.certificates {
            /*TODO: prints := c.GetFingerprints()?;
            fingerprints.push(prints);*/
        }

        Ok(DTLSParameters {
            role: DTLSRole::Auto, // always returns the default role
            fingerprints,
        })
    }

    /// get_remote_certificate returns the certificate chain in use by the remote side
    /// returns an empty list prior to selection of the remote certificate
    pub fn get_remote_certificate(&self) -> Bytes {
        self.remote_certificate.clone()
    }

    pub(crate) async fn start_srtp(&mut self) -> Result<()> {
        let mut srtp_config = srtp::config::Config {
            profile: self.srtp_protection_profile,
            ..Default::default()
        };
        let mut srtcp_config = srtp::config::Config {
            profile: self.srtp_protection_profile,
            ..Default::default()
        };

        if self.setting_engine.replay_protection.srtp != 0 {
            srtp_config.remote_rtp_options = Some(srtp::option::srtp_replay_protection(
                self.setting_engine.replay_protection.srtp,
            ));
        } else if self.setting_engine.disable_srtp_replay_protection {
            srtp_config.remote_rtp_options = Some(srtp::option::srtp_no_replay_protection());
        }

        if self.setting_engine.replay_protection.srtcp != 0 {
            srtcp_config.remote_rtcp_options = Some(srtp::option::srtcp_replay_protection(
                self.setting_engine.replay_protection.srtcp,
            ));
        } else if self.setting_engine.disable_srtcp_replay_protection {
            srtcp_config.remote_rtcp_options = Some(srtp::option::srtcp_no_replay_protection());
        }

        if let Some(conn) = &self.conn {
            let conn_state = conn.connection_state().await;
            srtp_config
                .extract_session_keys_from_dtls(conn_state, self.role() == DTLSRole::Client)
                .await?;
        } else {
            return Err(Error::ErrDtlsTransportNotStarted.into());
        }

        self.srtp_session = if let Some(srtp_endpoint) = &self.srtp_endpoint {
            Some(
                Session::new(
                    Arc::clone(srtp_endpoint) as Arc<dyn Conn + Send + Sync>,
                    srtp_config,
                    true,
                )
                .await?,
            )
        } else {
            None
        };

        self.srtcp_session = if let Some(srtcp_endpoint) = &self.srtcp_endpoint {
            Some(
                Session::new(
                    Arc::clone(&srtcp_endpoint) as Arc<dyn Conn + Send + Sync>,
                    srtcp_config,
                    false,
                )
                .await?,
            )
        } else {
            None
        };

        self.srtp_ready_tx.take();

        Ok(())
    }

    fn get_srtp_session(&self) -> Option<&Session> {
        self.srtp_session.as_ref()
    }

    fn get_srtcp_session(&self) -> Option<&Session> {
        self.srtcp_session.as_ref()
    }

    pub(crate) fn role(&self) -> DTLSRole {
        // If remote has an explicit role use the inverse
        match self.remote_parameters.role {
            DTLSRole::Client => return DTLSRole::Server,
            DTLSRole::Server => return DTLSRole::Client,
            _ => {}
        };

        // If SettingEngine has an explicit role
        match self.setting_engine.answering_dtls_role {
            DTLSRole::Server => return DTLSRole::Server,
            DTLSRole::Client => return DTLSRole::Client,
            _ => {}
        };

        // Remote was auto and no explicit role was configured via SettingEngine
        if self.ice_transport.role() == ICERole::Controlling {
            return DTLSRole::Server;
        }

        DEFAULT_DTLS_ROLE_ANSWER
    }

    async fn prepare_transport(
        &mut self,
        remote_parameters: DTLSParameters,
    ) -> Result<(DTLSRole, dtls::config::Config)> {
        self.ensure_ice_conn()?;

        if self.state != DTLSTransportState::New {
            return Err(Error::ErrInvalidDTLSStart.into());
        }

        self.srtp_endpoint = self.ice_transport.new_endpoint(Box::new(match_srtp)).await;
        self.srtcp_endpoint = self.ice_transport.new_endpoint(Box::new(match_srtcp)).await;
        self.remote_parameters = remote_parameters;

        let cert = self.certificates[0].clone();
        self.state_change(DTLSTransportState::Connecting).await;

        Ok((
            self.role(),
            dtls::config::Config {
                certificates: vec![cert],
                srtp_protection_profiles: if !self
                    .setting_engine
                    .srtp_protection_profiles
                    .is_empty()
                {
                    self.setting_engine.srtp_protection_profiles.clone()
                } else {
                    default_srtp_protection_profiles()
                },
                client_auth: ClientAuthType::RequireAnyClientCert,
                insecure_skip_verify: true,
                ..Default::default()
            },
        ))
    }

    /// start DTLS transport negotiation with the parameters of the remote DTLS transport
    pub async fn start(&mut self, remote_parameters: DTLSParameters) -> Result<()> {
        let dtls_conn_result = if let Some(dtls_endpoint) =
            self.ice_transport.new_endpoint(Box::new(match_dtls)).await
        {
            let (role, mut dtls_config) = self.prepare_transport(remote_parameters).await?;
            if self.setting_engine.replay_protection.dtls != 0 {
                dtls_config.replay_protection_window = self.setting_engine.replay_protection.dtls;
            }

            // Connect as DTLS Client/Server, function is blocking and we
            // must not hold the DTLSTransport lock
            if role == DTLSRole::Client {
                dtls::conn::DTLSConn::new(
                    dtls_endpoint as Arc<dyn Conn + Send + Sync>,
                    dtls_config,
                    true,
                    None,
                )
                .await
            } else {
                dtls::conn::DTLSConn::new(
                    dtls_endpoint as Arc<dyn Conn + Send + Sync>,
                    dtls_config,
                    false,
                    None,
                )
                .await
            }
        } else {
            Err(Error::new("ice_transport.new_endpoint failed".to_owned()).into())
        };

        let dtls_conn = match dtls_conn_result {
            Ok(dtls_conn) => dtls_conn,
            Err(err) => {
                self.state_change(DTLSTransportState::Failed).await;
                return Err(err);
            }
        };

        let srtp_profile = dtls_conn.selected_srtpprotection_profile();
        match srtp_profile {
            dtls::extension::extension_use_srtp::SrtpProtectionProfile::Srtp_Aead_Aes_128_Gcm=> {
                self.srtp_protection_profile = srtp::protection_profile::ProtectionProfile::AeadAes128Gcm;
            }
            dtls::extension::extension_use_srtp::SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80=> {
                self.srtp_protection_profile = srtp::protection_profile::ProtectionProfile::Aes128CmHmacSha1_80;
            }
            _=> {
                self.state_change(DTLSTransportState::Failed).await;
                return Err(Error::ErrNoSRTPProtectionProfile.into());
            }
        };

        if self
            .setting_engine
            .disable_certificate_fingerprint_verification
        {
            return Ok(());
        }

        // Check the fingerprint if a certificate was exchanged
        let remote_certs = &dtls_conn.connection_state().await.peer_certificates;
        if remote_certs.is_empty() {
            self.state_change(DTLSTransportState::Failed).await;
            return Err(Error::ErrNoRemoteCertificate.into());
        }
        self.remote_certificate = Bytes::from(remote_certs[0].clone());

        /*TODO: let parsedRemoteCert = x509.ParseCertificate(t.remote_certificate)
        if err != nil {
            if closeErr := dtlsConn.Close(); closeErr != nil {
                t.log.Error(err.Error())
            }

            t.onStateChange(DTLSTransportStateFailed)
            return err
        }

        if err = t.validate_finger_print(parsedRemoteCert); err != nil {
            if closeErr := dtlsConn.Close(); closeErr != nil {
                t.log.Error(err.Error())
            }

            t.onStateChange(DTLSTransportStateFailed)
            return err
        }*/

        self.conn = Some(Arc::new(dtls_conn));
        self.state_change(DTLSTransportState::Connected).await;

        self.start_srtp().await
    }

    /// stops and closes the DTLSTransport object.
    pub async fn stop(&mut self) -> Result<()> {
        // Try closing everything and collect the errors
        let mut close_errs: Vec<anyhow::Error> = vec![];
        if let Some(mut srtp_session) = self.srtp_session.take() {
            match srtp_session.close().await {
                Ok(_) => {}
                Err(err) => {
                    close_errs.push(err);
                }
            };
        }

        if let Some(mut srtcp_session) = self.srtcp_session.take() {
            match srtcp_session.close().await {
                Ok(_) => {}
                Err(err) => {
                    close_errs.push(err);
                }
            };
        }

        for ss in &mut self.simulcast_streams {
            match ss.close().await {
                Ok(_) => {}
                Err(err) => {
                    close_errs.push(err);
                }
            };
        }

        if let Some(conn) = self.conn.take() {
            // dtls_transport connection may be closed on sctp close.
            match conn.close().await {
                Ok(_) => {}
                Err(err) => {
                    if err.to_string() != dtls::error::Error::ErrConnClosed.to_string() {
                        close_errs.push(err);
                    }
                }
            }
        }

        self.state_change(DTLSTransportState::Closed).await;

        if close_errs.is_empty() {
            Ok(())
        } else {
            let close_errs_strs: Vec<String> =
                close_errs.into_iter().map(|e| e.to_string()).collect();
            Err(Error::new(close_errs_strs.join("\n")).into())
        }
    }

    pub(crate) fn validate_fingerprint(&self, _remote_cert: &[u8]) -> Result<()> {
        /*TODO: for  fp in self.remote_parameters.fingerprints {
            hashAlgo, err := fingerprint.HashFromString(fp.algorithm);
            if err != nil {
                return err
            }

            remoteValue, err := fingerprint.Fingerprint(remoteCert, hashAlgo)
            if err != nil {
                return err
            }

            if strings.EqualFold(remoteValue, fp.Value) {
                return nil
            }
        }

        return errNoMatchingCertificateFingerprint*/
        Ok(())
    }

    pub(crate) fn ensure_ice_conn(&self) -> Result<()> {
        if self.ice_transport.state() == ICETransportState::New {
            Err(Error::ErrICEConnectionNotStarted.into())
        } else {
            Ok(())
        }
    }

    pub(crate) fn store_simulcast_stream(&mut self, stream: Stream) {
        self.simulcast_streams.push(stream)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_invalid_fingerprint_causes_failed() -> Result<()> {
        //TODO:
        Ok(())
    }

    #[tokio::test]
    async fn test_peer_connection_dtls_role_setting_engine() -> Result<()> {
        //TODO:
        Ok(())
    }
}
