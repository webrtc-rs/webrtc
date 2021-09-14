#[cfg(test)]
mod api_test;

pub mod interceptor_registry;
pub mod media_engine;
pub mod setting_engine;

use crate::media::dtls_transport::dtls_certificate::Certificate;
use crate::media::dtls_transport::DTLSTransport;
use crate::media::ice_transport::ICETransport;
use crate::peer::ice::ice_gather::ice_gatherer::ICEGatherer;
use crate::peer::ice::ice_gather::ICEGatherOptions;

use media_engine::*;
use setting_engine::*;

use crate::data::data_channel::data_channel_parameters::DataChannelParameters;
use crate::data::data_channel::DataChannel;
use crate::data::sctp_transport::SCTPTransport;
use crate::error::Error;
use crate::media::rtp::rtp_codec::RTPCodecType;
use crate::media::rtp::rtp_receiver::RTPReceiver;
use crate::media::rtp::rtp_sender::RTPSender;
use crate::media::track::track_local::TrackLocal;
use crate::peer::configuration::Configuration;
use crate::peer::peer_connection::PeerConnection;
use interceptor::{noop::NoOp, registry::Registry, Interceptor};

use anyhow::Result;
use rcgen::KeyPair;
use std::sync::Arc;
use std::time::SystemTime;

/// API bundles the global functions of the WebRTC and ORTC API.
/// Some of these functions are also exported globally using the
/// defaultAPI object. Note that the global version of the API
/// may be phased out in the future.
pub struct API {
    pub(crate) setting_engine: Arc<SettingEngine>,
    pub(crate) media_engine: Arc<MediaEngine>,
    pub(crate) interceptor: Arc<dyn Interceptor + Send + Sync>,
}

impl API {
    /// new_peer_connection creates a new PeerConnection with the provided configuration against the received API object
    pub async fn new_peer_connection(
        &self,
        configuration: Configuration,
    ) -> Result<PeerConnection> {
        PeerConnection::new(self, configuration).await
    }

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
            Arc::clone(&self.setting_engine),
        ))
    }

    /// new_ice_transport creates a new ice transport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub fn new_ice_transport(&self, gatherer: Arc<ICEGatherer>) -> ICETransport {
        ICETransport::new(gatherer)
    }

    /// new_dtls_transport creates a new dtls_transport transport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub fn new_dtls_transport(
        &self,
        ice_transport: Arc<ICETransport>,
        mut certificates: Vec<Certificate>,
    ) -> Result<DTLSTransport> {
        if !certificates.is_empty() {
            let now = SystemTime::now();
            for cert in &certificates {
                if cert.expires().duration_since(now).is_err() {
                    return Err(Error::ErrCertificateExpired.into());
                }
            }
        } else {
            let kp = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
            let cert = Certificate::from_key_pair(kp)?;
            certificates = vec![cert];
        };

        Ok(DTLSTransport::new(
            ice_transport,
            certificates,
            Arc::clone(&self.setting_engine),
        ))
    }

    /// new_sctp_transport creates a new SCTPTransport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub fn new_sctp_transport(&self, dtls_transport: Arc<DTLSTransport>) -> Result<SCTPTransport> {
        Ok(SCTPTransport::new(
            dtls_transport,
            Arc::clone(&self.setting_engine),
        ))
    }

    /// new_data_channel creates a new DataChannel.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub async fn new_data_channel(
        sctp_transport: Arc<SCTPTransport>,
        params: DataChannelParameters,
        setting_engine: Arc<SettingEngine>,
    ) -> Result<DataChannel> {
        // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #5)
        if params.label.len() > 65535 {
            return Err(Error::ErrStringSizeLimit.into());
        }

        let d = DataChannel::new(params, setting_engine);
        d.open(sctp_transport).await?;

        Ok(d)
    }

    /// new_rtp_receiver constructs a new RTPReceiver
    pub fn new_rtp_receiver(
        &self,
        kind: RTPCodecType,
        transport: Arc<DTLSTransport>,
    ) -> RTPReceiver {
        RTPReceiver::new(
            kind,
            transport,
            Arc::clone(&self.media_engine),
            Arc::clone(&self.interceptor),
        )
    }

    /// new_rtp_sender constructs a new RTPSender
    pub async fn new_rtp_sender(
        &self,
        track: Arc<dyn TrackLocal + Send + Sync>,
        transport: Arc<DTLSTransport>,
    ) -> RTPSender {
        RTPSender::new(
            track,
            transport,
            Arc::clone(&self.media_engine),
            Arc::clone(&self.interceptor),
        )
        .await
    }
}

#[derive(Default)]
pub struct APIBuilder {
    setting_engine: Option<Arc<SettingEngine>>,
    media_engine: Option<Arc<MediaEngine>>,
    interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,
}

impl APIBuilder {
    pub fn new() -> Self {
        APIBuilder::default()
    }

    pub fn build(mut self) -> API {
        API {
            setting_engine: if let Some(setting_engine) = self.setting_engine.take() {
                setting_engine
            } else {
                Arc::new(SettingEngine::default())
            },
            media_engine: if let Some(media_engine) = self.media_engine.take() {
                media_engine
            } else {
                Arc::new(MediaEngine::default())
            },
            interceptor: if let Some(interceptor) = self.interceptor.take() {
                interceptor
            } else {
                Arc::new(NoOp {})
            },
        }
    }

    /// WithSettingEngine allows providing a SettingEngine to the API.
    /// Settings should not be changed after passing the engine to an API.
    pub fn with_setting_engine(mut self, setting_engine: SettingEngine) -> Self {
        self.setting_engine = Some(Arc::new(setting_engine));
        self
    }

    /// WithMediaEngine allows providing a MediaEngine to the API.
    /// Settings can be changed after passing the engine to an API.
    pub fn with_media_engine(mut self, media_engine: MediaEngine) -> Self {
        self.media_engine = Some(Arc::new(media_engine));
        self
    }

    /// with_interceptor_registry allows providing Interceptors to the API.
    /// Settings should not be changed after passing the registry to an API.
    pub fn with_interceptor_registry(mut self, interceptor_registry: Registry) -> Self {
        self.interceptor = Some(interceptor_registry.build());
        self
    }
}
