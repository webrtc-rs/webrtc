#[cfg(test)]
mod api_test;

pub mod interceptor_registry;
pub mod media_engine;
pub mod setting_engine;

use crate::dtls_transport::RTCDtlsTransport;
use crate::ice_transport::ice_gatherer::RTCIceGatherOptions;
use crate::ice_transport::ice_gatherer::RTCIceGatherer;
use crate::ice_transport::RTCIceTransport;
use crate::peer_connection::certificate::RTCCertificate;

use media_engine::*;
use setting_engine::*;

use crate::data_channel::data_channel_parameters::DataChannelParameters;
use crate::data_channel::RTCDataChannel;
use crate::error::{Error, Result};
use crate::peer_connection::configuration::RTCConfiguration;
use crate::peer_connection::RTCPeerConnection;
use crate::rtp_transceiver::rtp_codec::RTPCodecType;
use crate::rtp_transceiver::rtp_receiver::RTCRtpReceiver;
use crate::rtp_transceiver::rtp_sender::RTCRtpSender;
use crate::sctp_transport::RTCSctpTransport;
use crate::track::track_local::TrackLocal;
use interceptor::{registry::Registry, Interceptor};

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
    pub(crate) interceptor_registry: Registry,
}

impl API {
    /// new_peer_connection creates a new PeerConnection with the provided configuration against the received API object
    pub async fn new_peer_connection(
        &self,
        configuration: RTCConfiguration,
    ) -> Result<RTCPeerConnection> {
        RTCPeerConnection::new(self, configuration).await
    }

    /// new_ice_gatherer creates a new ice gatherer.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub fn new_ice_gatherer(&self, opts: RTCIceGatherOptions) -> Result<RTCIceGatherer> {
        let mut validated_servers = vec![];
        if !opts.ice_servers.is_empty() {
            for server in &opts.ice_servers {
                let url = server.urls()?;
                validated_servers.extend(url);
            }
        }

        Ok(RTCIceGatherer::new(
            validated_servers,
            opts.ice_gather_policy,
            Arc::clone(&self.setting_engine),
        ))
    }

    /// new_ice_transport creates a new ice transport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub fn new_ice_transport(&self, gatherer: Arc<RTCIceGatherer>) -> RTCIceTransport {
        RTCIceTransport::new(gatherer)
    }

    /// new_dtls_transport creates a new dtls_transport transport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub fn new_dtls_transport(
        &self,
        ice_transport: Arc<RTCIceTransport>,
        mut certificates: Vec<RTCCertificate>,
    ) -> Result<RTCDtlsTransport> {
        if !certificates.is_empty() {
            let now = SystemTime::now();
            for cert in &certificates {
                cert.expires
                    .duration_since(now)
                    .map_err(|_| Error::ErrCertificateExpired)?;
            }
        } else {
            let kp = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
            let cert = RTCCertificate::from_key_pair(kp)?;
            certificates = vec![cert];
        };

        Ok(RTCDtlsTransport::new(
            ice_transport,
            certificates,
            Arc::clone(&self.setting_engine),
        ))
    }

    /// new_sctp_transport creates a new SCTPTransport.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub fn new_sctp_transport(
        &self,
        dtls_transport: Arc<RTCDtlsTransport>,
    ) -> Result<RTCSctpTransport> {
        Ok(RTCSctpTransport::new(
            dtls_transport,
            Arc::clone(&self.setting_engine),
        ))
    }

    /// new_data_channel creates a new DataChannel.
    /// This constructor is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    pub async fn new_data_channel(
        &self,
        sctp_transport: Arc<RTCSctpTransport>,
        params: DataChannelParameters,
    ) -> Result<RTCDataChannel> {
        // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #5)
        if params.label.len() > 65535 {
            return Err(Error::ErrStringSizeLimit);
        }

        let d = RTCDataChannel::new(params, Arc::clone(&self.setting_engine));
        d.open(sctp_transport).await?;

        Ok(d)
    }

    /// new_rtp_receiver constructs a new RTPReceiver
    pub fn new_rtp_receiver(
        &self,
        kind: RTPCodecType,
        transport: Arc<RTCDtlsTransport>,
        interceptor: Arc<dyn Interceptor + Send + Sync>,
    ) -> RTCRtpReceiver {
        RTCRtpReceiver::new(
            self.setting_engine.get_receive_mtu(),
            kind,
            transport,
            Arc::clone(&self.media_engine),
            interceptor,
        )
    }

    /// new_rtp_sender constructs a new RTPSender
    pub async fn new_rtp_sender(
        &self,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
        transport: Arc<RTCDtlsTransport>,
        interceptor: Arc<dyn Interceptor + Send + Sync>,
    ) -> RTCRtpSender {
        RTCRtpSender::new(
            self.setting_engine.get_receive_mtu(),
            track,
            transport,
            Arc::clone(&self.media_engine),
            interceptor,
            false,
        )
        .await
    }

    /// Returns the internal [`SettingEngine`].
    pub fn setting_engine(&self) -> Arc<SettingEngine> {
        Arc::clone(&self.setting_engine)
    }

    /// Returns the internal [`MediaEngine`].
    pub fn media_engine(&self) -> Arc<MediaEngine> {
        Arc::clone(&self.media_engine)
    }
}

#[derive(Default)]
pub struct APIBuilder {
    setting_engine: Option<Arc<SettingEngine>>,
    media_engine: Option<Arc<MediaEngine>>,
    interceptor_registry: Option<Registry>,
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
            interceptor_registry: if let Some(interceptor_registry) =
                self.interceptor_registry.take()
            {
                interceptor_registry
            } else {
                Registry::new()
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
        self.interceptor_registry = Some(interceptor_registry);
        self
    }
}
