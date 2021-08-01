use crate::media::dtls_transport::DTLSTransport;
use crate::media::ice_transport::ICETransport;
use crate::peer::ice::ice_gather::ice_gatherer::ICEGatherer;
use crate::peer::ice::ice_gather::ICEGatherOptions;

use dtls::crypto::Certificate;
use media_engine::*;
use setting_engine::*;

pub mod media_engine;
pub mod setting_engine;

use crate::data::data_channel::data_channel_parameters::DataChannelParameters;
use crate::data::data_channel::DataChannel;
use crate::data::sctp_transport::SCTPTransport;
use crate::error::Error;
use crate::media::interceptor::Interceptor;
use crate::media::rtp::rtp_codec::RTPCodecType;
use crate::media::rtp::rtp_receiver::RTPReceiver;

use crate::media::interceptor::stream_info::StreamInfo;
use crate::media::rtp::rtp_sender::RTPSender;
use crate::media::rtp::srtp_writer_future::SrtpWriterFuture;
use crate::media::track::track_local::{TrackLocal, TrackLocalContext};
use crate::peer::configuration::Configuration;
use crate::peer::peer_connection::PeerConnection;
use anyhow::Result;
use ice::rand::generate_crypto_random_string;
use std::sync::Arc;
use tokio::sync::mpsc;

/// API bundles the global functions of the WebRTC and ORTC API.
/// Some of these functions are also exported globally using the
/// defaultAPI object. Note that the global version of the API
/// may be phased out in the future.
pub struct API {
    pub(crate) setting_engine: Arc<SettingEngine>,
    pub(crate) media_engine: Arc<MediaEngine>,
    pub(crate) interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,
}

impl API {
    /// new_peer_connection creates a new PeerConnection with the provided configuration against the received API object
    pub async fn new_peer_connection(
        &self,
        configuration: Configuration,
    ) -> Result<Arc<PeerConnection>> {
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
        &self,
        sctp_transport: Arc<SCTPTransport>,
        params: DataChannelParameters,
    ) -> Result<DataChannel> {
        // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #5)
        if params.label.len() > 65535 {
            return Err(Error::ErrStringSizeLimit.into());
        }

        let d = DataChannel::new(params, Arc::clone(&self.setting_engine));
        d.open(sctp_transport).await?;

        Ok(d)
    }

    /// new_rtp_receiver constructs a new RTPReceiver
    pub fn new_rtp_receiver(
        &self,
        kind: RTPCodecType,
        transport: Arc<DTLSTransport>,
    ) -> RTPReceiver {
        let (closed_tx, closed_rx) = mpsc::channel(1);
        let (received_tx, received_rx) = mpsc::channel(1);

        RTPReceiver {
            kind,
            transport,

            tracks: vec![],

            closed_tx: Some(closed_tx),
            closed_rx,
            received_tx: Some(received_tx),
            received_rx,

            media_engine: Arc::clone(&self.media_engine),
            interceptor: self.interceptor.clone(),
        }
    }

    /// new_rtp_sender constructs a new RTPSender
    pub fn new_rtp_sender(
        &self,
        track: Arc<dyn TrackLocal + Send + Sync>,
        transport: Arc<DTLSTransport>,
    ) -> RTPSender {
        let id = generate_crypto_random_string(
            32,
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
        );
        let (send_called_tx, send_called_rx) = mpsc::channel(1);
        let (stop_called_tx, stop_called_rx) = mpsc::channel(1);
        let ssrc = rand::random::<u32>();
        let srtp_stream = SrtpWriterFuture::default();

        RTPSender {
            track: Some(track),

            srtp_stream,
            rtcp_interceptor: None,
            stream_info: StreamInfo::default(),

            context: TrackLocalContext::default(),
            transport,

            payload_type: 0,
            ssrc,

            negotiated: false,

            media_engine: Arc::clone(&self.media_engine),
            interceptor: self.interceptor.clone(),

            id,

            //api:        api,
            send_called_tx: Some(send_called_tx),
            send_called_rx,
            stop_called_tx: Some(stop_called_tx),
            stop_called_rx,
        }

        /*TODO: r.srtp_stream.rtpSender = r

        r.rtcp_interceptor = r.api.interceptor.bind_rtcpreader(interceptor.RTPReaderFunc(func(in []byte, a interceptor.Attributes) (n int, attributes interceptor.Attributes, err error) {
            n, err = r.srtp_stream.Read(in)
            return n, a, err
        }))

        return r, nil*/
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
            interceptor: self.interceptor.take(),
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

    /// with_interceptor allows providing Interceptors to the API.
    /// Settings should not be changed after passing the registry to an API.
    pub fn with_interceptor(mut self, interceptor: Arc<dyn Interceptor + Send + Sync>) -> Self {
        self.interceptor = Some(interceptor);
        self
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_api() -> Result<()> {
        let mut s = SettingEngine::default();
        s.detach_data_channels();
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;

        let api = APIBuilder::new()
            .with_setting_engine(s)
            .with_media_engine(m)
            .build();

        assert_eq!(
            api.setting_engine.detach.data_channels, true,
            "Failed to set settings engine"
        );
        assert_eq!(
            api.media_engine.audio_codecs.is_empty(),
            false,
            "Failed to set media engine"
        );

        Ok(())
    }
}
