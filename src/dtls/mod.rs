pub mod dtls_role;
pub mod dtls_transport_state;

use dtls_role::*;

use crate::api::setting_engine::SettingEngine;
use crate::dtls::dtls_transport_state::DTLSTransportState;
use crate::error::Error;
use crate::ice::ice_role::ICERole;
use crate::ice::ice_transport::ice_transport_state::ICETransportState;
use crate::ice::ice_transport::ICETransport;
use crate::mux::endpoint::Endpoint;
use crate::mux::mux_func::MatchFunc;
use bytes::Bytes;
use dtls::conn::DTLSConn;
use dtls::crypto::Certificate;
use serde::{Deserialize, Serialize};
use srtp::protection_profile::ProtectionProfile;
use srtp::session::Session;
use srtp::stream::Stream;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use util::Conn;

/// DTLSFingerprint specifies the hash function algorithm and certificate
/// fingerprint as described in https://tools.ietf.org/html/rfc4572.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct DTLSFingerprint {
    /// Algorithm specifies one of the the hash function algorithms defined in
    /// the 'Hash function Textual Names' registry.
    pub algorithm: String,

    /// Value specifies the value of the certificate fingerprint in lowercase
    /// hex string as expressed utilizing the syntax of 'fingerprint' in
    /// https://tools.ietf.org/html/rfc4572#section-5.
    pub value: String,
}

/// DTLSParameters holds information relating to DTLS configuration.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct DTLSParameters {
    pub role: DTLSRole,
    pub fingerprints: Vec<DTLSFingerprint>,
}

pub type OnStateChangeHdlrFn = Box<
    dyn (FnMut(DTLSTransportState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

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
    conn: DTLSConn,

    srtp_session: Option<Session>,
    srtcp_session: Option<Session>,
    srtp_endpoint: Arc<Endpoint>,
    srtcp_endpoint: Arc<Endpoint>,

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

        let conn_state = self.conn.connection_state().await;
        srtp_config
            .extract_session_keys_from_dtls(conn_state, self.role() == DTLSRole::Client)
            .await?;

        let srtp_session = Session::new(
            Arc::clone(&self.srtp_endpoint) as Arc<dyn Conn + Send + Sync>,
            srtp_config,
            true,
        )
        .await?;

        let srtcp_session = Session::new(
            Arc::clone(&self.srtcp_endpoint) as Arc<dyn Conn + Send + Sync>,
            srtcp_config,
            false,
        )
        .await?;

        self.srtp_session = Some(srtp_session);
        self.srtcp_session = Some(srtcp_session);
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
    /*
        // Start DTLS transport negotiation with the parameters of the remote DTLS transport
        func (t *DTLSTransport) Start(remote_parameters DTLSParameters) error {
            // Take lock and prepare connection, we must not hold the lock
            // when connecting
            prepareTransport := func() (DTLSRole, *dtls.Config, error) {
                t.lock.Lock()
                defer t.lock.Unlock()

                if err := t.ensure_iceconn(); err != nil {
                    return DTLSRole(0), nil, err
                }

                if t.state != DTLSTransportStateNew {
                    return DTLSRole(0), nil, &rtcerr.InvalidStateError{Err: fmt.Errorf("%w: %s", errInvalidDTLSStart, t.state)}
                }

                t.srtp_endpoint = t.ice_transport.newEndpoint(mux.MatchSRTP)
                t.srtcpEndpoint = t.ice_transport.newEndpoint(mux.MatchSRTCP)
                t.remote_parameters = remote_parameters

                cert := t.certificates[0]
                t.onStateChange(DTLSTransportStateConnecting)

                return t.role(), &dtls.Config{
                    Certificates: []tls.Certificate{
                        {
                            Certificate: [][]byte{cert.x509Cert.Raw},
                            PrivateKey:  cert.privateKey,
                        },
                    },
                    SRTPProtectionProfiles: func() []dtls.SRTPProtectionProfile {
                        if len(t.api.settingEngine.srtpProtectionProfiles) > 0 {
                            return t.api.settingEngine.srtpProtectionProfiles
                        }

                        return defaultSrtpProtectionProfiles()
                    }(),
                    ClientAuth:         dtls.RequireAnyClientCert,
                    LoggerFactory:      t.api.settingEngine.LoggerFactory,
                    InsecureSkipVerify: true,
                }, nil
            }

            var dtlsConn *dtls.Conn
            dtlsEndpoint := t.ice_transport.newEndpoint(mux.MatchDTLS)
            role, dtlsConfig, err := prepareTransport()
            if err != nil {
                return err
            }

            if t.api.settingEngine.replayProtection.DTLS != nil {
                dtlsConfig.ReplayProtectionWindow = int(*t.api.settingEngine.replayProtection.DTLS)
            }

            // Connect as DTLS Client/Server, function is blocking and we
            // must not hold the DTLSTransport lock
            if role == DTLSRoleClient {
                dtlsConn, err = dtls.Client(dtlsEndpoint, dtlsConfig)
            } else {
                dtlsConn, err = dtls.Server(dtlsEndpoint, dtlsConfig)
            }

            // Re-take the lock, nothing beyond here is blocking
            t.lock.Lock()
            defer t.lock.Unlock()

            if err != nil {
                t.onStateChange(DTLSTransportStateFailed)
                return err
            }

            srtpProfile, ok := dtlsConn.SelectedSRTPProtectionProfile()
            if !ok {
                t.onStateChange(DTLSTransportStateFailed)
                return ErrNoSRTPProtectionProfile
            }

            switch srtpProfile {
            case dtls.SRTP_AEAD_AES_128_GCM:
                t.srtp_protection_profile = srtp.ProtectionProfileAeadAes128Gcm
            case dtls.SRTP_AES128_CM_HMAC_SHA1_80:
                t.srtp_protection_profile = srtp.ProtectionProfileAes128CmHmacSha1_80
            default:
                t.onStateChange(DTLSTransportStateFailed)
                return ErrNoSRTPProtectionProfile
            }

            if t.api.settingEngine.disableCertificateFingerprintVerification {
                return nil
            }

            // Check the fingerprint if a certificate was exchanged
            remoteCerts := dtlsConn.ConnectionState().PeerCertificates
            if len(remoteCerts) == 0 {
                t.onStateChange(DTLSTransportStateFailed)
                return errNoRemoteCertificate
            }
            t.remote_certificate = remoteCerts[0]

            parsedRemoteCert, err := x509.ParseCertificate(t.remote_certificate)
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
            }

            t.conn = dtlsConn
            t.onStateChange(DTLSTransportStateConnected)

            return t.startSRTP()
        }

        // Stop stops and closes the DTLSTransport object.
        func (t *DTLSTransport) Stop() error {
            t.lock.Lock()
            defer t.lock.Unlock()

            // Try closing everything and collect the errors
            var closeErrs []error

            if srtpSessionValue := t.srtp_session.Load(); srtpSessionValue != nil {
                closeErrs = append(closeErrs, srtpSessionValue.(*srtp.SessionSRTP).Close())
            }

            if srtcpSessionValue := t.srtcp_session.Load(); srtcpSessionValue != nil {
                closeErrs = append(closeErrs, srtcpSessionValue.(*srtp.SessionSRTCP).Close())
            }

            for i := range t.simulcast_streams {
                closeErrs = append(closeErrs, t.simulcast_streams[i].Close())
            }

            if t.conn != nil {
                // dtls connection may be closed on sctp close.
                if err := t.conn.Close(); err != nil && !errors.Is(err, dtls.ErrConnClosed) {
                    closeErrs = append(closeErrs, err)
                }
            }
            t.onStateChange(DTLSTransportStateClosed)
            return util.FlattenErrs(closeErrs)
        }
    */
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
