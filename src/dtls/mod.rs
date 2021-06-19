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
use dtls::extension::extension_use_srtp::SrtpProtectionProfile;
use serde::{Deserialize, Serialize};
use srtp::session::Session;
use srtp::stream::Stream;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

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
    srtp_protection_profile: SrtpProtectionProfile,
    on_state_change_handler: Arc<Mutex<Option<OnStateChangeHdlrFn>>>,
    conn: DTLSConn,

    srtp_session: Option<Session>, //atomic.Value
    srtcp_session: Option<Session>,
    srtp_endpoint: Arc<Endpoint>,
    srctp_endpoint: Arc<Endpoint>,

    simulcast_streams: Vec<Stream>,
    srtp_ready: mpsc::Receiver<()>,

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
    /*
                func (t *DTLSTransport) startSRTP() error {
                    srtpConfig := &srtp.Config{
                        Profile:       t.srtp_protection_profile,
                        BufferFactory: t.api.settingEngine.BufferFactory,
                        LoggerFactory: t.api.settingEngine.LoggerFactory,
                    }
                    if t.api.settingEngine.replayProtection.SRTP != nil {
                        srtpConfig.RemoteOptions = append(
                            srtpConfig.RemoteOptions,
                            srtp.SRTPReplayProtection(*t.api.settingEngine.replayProtection.SRTP),
                        )
                    }

                    if t.api.settingEngine.disableSRTPReplayProtection {
                        srtpConfig.RemoteOptions = append(
                            srtpConfig.RemoteOptions,
                            srtp.SRTPNoReplayProtection(),
                        )
                    }

                    if t.api.settingEngine.replayProtection.SRTCP != nil {
                        srtpConfig.RemoteOptions = append(
                            srtpConfig.RemoteOptions,
                            srtp.SRTCPReplayProtection(*t.api.settingEngine.replayProtection.SRTCP),
                        )
                    }

                    if t.api.settingEngine.disableSRTCPReplayProtection {
                        srtpConfig.RemoteOptions = append(
                            srtpConfig.RemoteOptions,
                            srtp.SRTCPNoReplayProtection(),
                        )
                    }

                    connState := t.conn.ConnectionState()
                    err := srtpConfig.ExtractSessionKeysFromDTLS(&connState, t.role() == DTLSRoleClient)
                    if err != nil {
                        return fmt.Errorf("%w: %v", errDtlsKeyExtractionFailed, err)
                    }

                    srtp_session, err := srtp.NewSessionSRTP(t.srtp_endpoint, srtpConfig)
                    if err != nil {
                        return fmt.Errorf("%w: %v", errFailedToStartSRTP, err)
                    }

                    srtcp_session, err := srtp.NewSessionSRTCP(t.srtcpEndpoint, srtpConfig)
                    if err != nil {
                        return fmt.Errorf("%w: %v", errFailedToStartSRTCP, err)
                    }

                    t.srtp_session.Store(srtp_session)
                    t.srtcp_session.Store(srtcp_session)
                    close(t.srtp_ready)
                    return nil
                }
    */
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
