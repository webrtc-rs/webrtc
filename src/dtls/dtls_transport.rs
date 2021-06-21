use super::*;

/// DTLSTransport allows an application access to information about the DTLS
/// transport over which RTP and RTCP packets are sent and received by
/// RTPSender and RTPReceiver, as well other data such as SCTP packets sent
/// and received by data channels.
pub struct DTLSTransport {
    ice_transport: Option<Arc<ICETransport>>,
    certificates: Vec<Certificate>,
    remote_parameters: DTLSParameters,
    remote_certificate: Bytes,
    state: DTLSTransportState,
    srtp_protection_profile: ProtectionProfile,
    on_state_change_handler: Arc<Mutex<Option<OnStateChangeHdlrFn>>>,
    conn: Option<DTLSConn>,

    srtp_session: Option<Session>,
    srtcp_session: Option<Session>,
    srtp_endpoint: Option<Arc<Endpoint>>,
    srtcp_endpoint: Option<Arc<Endpoint>>,

    simulcast_streams: Vec<Stream>,
    srtp_ready_tx: Option<mpsc::Sender<()>>,

    dtls_matcher: MatchFunc,
    setting_engine: SettingEngine,
}

impl DTLSTransport {
    /// returns the currently-configured ICETransport or None
    /// if one has not been configured
    pub fn ice_transport(&self) -> Option<Arc<ICETransport>> {
        self.ice_transport.clone()
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

    /// state returns the current dtls transport state.
    pub fn state(&self) -> DTLSTransportState {
        self.state
    }

    /// write_rtcp sends a user provided RTCP packet to the connected peer. If no peer is connected the
    /// packet is discarded.
    pub async fn write_rtcp(&mut self, pkt: &(dyn rtcp::packet::Packet)) -> Result<usize, Error> {
        if let Some(srtcp_session) = &mut self.srtcp_session {
            Ok(srtcp_session.write_rtcp(pkt).await?)
        } else {
            Ok(0)
        }
    }

    /// get_local_parameters returns the DTLS parameters of the local DTLSTransport upon construction.
    pub fn get_local_parameters(&self) -> Result<DTLSParameters, Error> {
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

    pub(crate) async fn start_srtp(&mut self) -> Result<(), Error> {
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
            return Err(Error::ErrDtlsTransportNotStarted);
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
        if let Some(ice_transport) = &self.ice_transport {
            if ice_transport.role() == ICERole::Controlling {
                return DTLSRole::Server;
            }
        }

        DEFAULT_DTLS_ROLE_ANSWER
    }

    async fn prepare_transport(
        &mut self,
        remote_parameters: DTLSParameters,
    ) -> Result<(DTLSRole, dtls::config::Config), Error> {
        self.ensure_ice_conn()?;

        if self.state != DTLSTransportState::New {
            return Err(Error::ErrInvalidDTLSStart);
        }

        if let Some(ice_transport) = &self.ice_transport {
            self.srtp_endpoint = ice_transport.new_endpoint(Box::new(match_srtp)).await;
            self.srtcp_endpoint = ice_transport.new_endpoint(Box::new(match_srtcp)).await;
        }
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
    pub async fn start(&mut self, remote_parameters: DTLSParameters) -> Result<(), Error> {
        let dtls_conn_result = if let Some(ice_transport) = &self.ice_transport {
            if let Some(dtls_endpoint) = ice_transport.new_endpoint(Box::new(match_dtls)).await {
                let (role, mut dtls_config) = self.prepare_transport(remote_parameters).await?;
                if self.setting_engine.replay_protection.dtls != 0 {
                    dtls_config.replay_protection_window =
                        self.setting_engine.replay_protection.dtls;
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
                Err(dtls::error::Error::ErrOthers(
                    "ice_transport.new_endpoint failed".to_owned(),
                ))
            }
        } else {
            Err(dtls::error::Error::ErrOthers(
                "ice_transport is None".to_owned(),
            ))
        };

        let dtls_conn = match dtls_conn_result {
            Ok(dtls_conn) => dtls_conn,
            Err(err) => {
                self.state_change(DTLSTransportState::Failed).await;
                return Err(err.into());
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
                return Err(Error::ErrNoSRTPProtectionProfile);
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
            return Err(Error::ErrNoRemoteCertificate);
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

        self.conn = Some(dtls_conn);
        self.state_change(DTLSTransportState::Connected).await;

        self.start_srtp().await
    }

    /// stops and closes the DTLSTransport object.
    pub async fn stop(&mut self) -> Result<(), Error> {
        // Try closing everything and collect the errors
        let mut close_errs: Vec<Error> = vec![];
        if let Some(mut srtp_session) = self.srtp_session.take() {
            match srtp_session.close().await {
                Ok(_) => {}
                Err(err) => {
                    close_errs.push(err.into());
                }
            };
        }

        if let Some(mut srtcp_session) = self.srtcp_session.take() {
            match srtcp_session.close().await {
                Ok(_) => {}
                Err(err) => {
                    close_errs.push(err.into());
                }
            };
        }

        for ss in &mut self.simulcast_streams {
            match ss.close().await {
                Ok(_) => {}
                Err(err) => {
                    close_errs.push(err.into());
                }
            };
        }

        if let Some(mut conn) = self.conn.take() {
            // dtls connection may be closed on sctp close.
            match conn.close().await {
                Ok(_) => {}
                Err(err) => {
                    if err.to_string() != dtls::error::Error::ErrConnClosed.to_string() {
                        close_errs.push(err.into());
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
            Err(Error::ErrOthers(close_errs_strs.join("\n")))
        }
    }

    pub(crate) fn validate_fingerprint(&self, _remote_cert: &[u8]) -> Result<(), Error> {
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

    pub(crate) fn ensure_ice_conn(&self) -> Result<(), Error> {
        if let Some(ice_transport) = &self.ice_transport {
            if ice_transport.state() == ICETransportState::New {
                Err(Error::ErrICEConnectionNotStarted)
            } else {
                Ok(())
            }
        } else {
            Err(Error::ErrICEConnectionNotStarted)
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
    async fn test_invalid_fingerprint_causes_failed() -> Result<(), Error> {
        //TODO:
        Ok(())
    }

    #[tokio::test]
    async fn test_peer_connection_dtls_role_setting_engine() -> Result<(), Error> {
        //TODO:
        Ok(())
    }
}
