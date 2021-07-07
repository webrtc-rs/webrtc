use crate::dtls::dtls_transport::DTLSTransport;
use crate::ice::ice_gather::ice_gatherer::ICEGatherer;
use crate::ice::ice_gather::ICEGatherOptions;
use crate::ice::ice_transport::ICETransport;

use dtls::crypto::Certificate;
use media_engine::*;
use setting_engine::*;

pub mod media_engine;
pub mod setting_engine;

use crate::data::data_channel::DataChannel;
use crate::data::DataChannelParameters;
use crate::error::Error;
use crate::sctp::sctp_transport::SCTPTransport;
use anyhow::Result;
use std::sync::Arc;

/// API bundles the global functions of the WebRTC and ORTC API.
/// Some of these functions are also exported globally using the
/// defaultAPI object. Note that the global version of the API
/// may be phased out in the future.
pub struct Api {
    setting_engine: SettingEngine,
    media_engine: MediaEngine,
    //TODO: interceptor   interceptor.Interceptor
}

impl Api {
    /// new_ice_gatherer creates a new ice gatherer.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub fn new_ice_gatherer(&self, opts: ICEGatherOptions) -> Result<ICEGatherer> {
        let mut validated_servers = vec![];
        if !opts.ice_servers.is_empty() {
            for server in &opts.ice_servers {
                let url = server.urls()?;
                validated_servers.extend(url);
            }
        }

        Ok(ICEGatherer::new(
            validated_servers,
            opts.ice_gather_policy,
            self.setting_engine.clone(),
        ))
    }

    /// new_ice_transport creates a new ice transport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub fn new_ice_transport(&self, gatherer: ICEGatherer) -> Result<ICETransport> {
        Ok(ICETransport::new(gatherer))
    }

    /// new_dtls_transport creates a new dtls transport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub fn new_dtls_transport(
        &self,
        ice_transport: ICETransport,
        certificates: Vec<Certificate>,
    ) -> Result<DTLSTransport> {
        /*TODO: if !certificates.is_empty() {
            now := time.Now()
            for _, x509Cert := range certificates {
                if !x509Cert.Expires().IsZero() && now.After(x509Cert.Expires()) {
                    return nil, &rtcerr.InvalidAccessError{Err: ErrCertificateExpired}
                }
                t.certificates = append(t.certificates, x509Cert)
            }
        } else {
            sk, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
            if err != nil {
                return nil, &rtcerr.UnknownError{Err: err}
            }
            certificate, err := GenerateCertificate(sk)
            if err != nil {
                return nil, err
            }
            t.certificates = []Certificate{*certificate}
        }*/

        Ok(DTLSTransport::new(
            ice_transport,
            certificates,
            self.setting_engine.clone(),
        ))
    }

    /// new_sctp_transport creates a new SCTPTransport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub fn new_sctp_transport(&self, dtls_transport: Arc<DTLSTransport>) -> Result<SCTPTransport> {
        Ok(SCTPTransport::new(
            dtls_transport,
            self.setting_engine.clone(),
        ))
    }

    /// new_data_channel creates a new DataChannel.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub async fn new_data_channel(
        &self,
        sctp_transport: Arc<SCTPTransport>,
        params: DataChannelParameters,
    ) -> Result<DataChannel> {
        // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #5)
        if params.label.len() > 65535 {
            return Err(Error::ErrStringSizeLimit.into());
        }

        let d = DataChannel::new(params, self.setting_engine.clone());
        d.open(sctp_transport).await?;

        Ok(d)
    }
}

pub struct ApiBuilder {
    api: Api,
}

impl Default for ApiBuilder {
    fn default() -> Self {
        ApiBuilder {
            api: Api {
                setting_engine: SettingEngine::default(),
                media_engine: MediaEngine::default(),
            },
        }
    }
}

impl ApiBuilder {
    pub fn new() -> Self {
        ApiBuilder::default()
    }

    pub fn build(self) -> Api {
        self.api
    }

    /// WithSettingEngine allows providing a SettingEngine to the API.
    /// Settings should not be changed after passing the engine to an API.
    pub fn with_setting_engine(mut self, setting_engine: SettingEngine) -> Self {
        self.api.setting_engine = setting_engine;
        self
    }

    /// WithMediaEngine allows providing a MediaEngine to the API.
    /// Settings can be changed after passing the engine to an API.
    pub fn with_media_engine(mut self, media_engine: MediaEngine) -> Self {
        self.api.media_engine = media_engine;
        self
    }

    //TODO:
    // WithInterceptorRegistry allows providing Interceptors to the API.
    // Settings should not be changed after passing the registry to an API.
    /*pub WithInterceptorRegistry(interceptorRegistry *interceptor.Registry) func(a *API) {
        return func(a *API) {
            a.interceptor = interceptorRegistry.Build()
        }
    }*/
}
