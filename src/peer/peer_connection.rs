#[cfg(test)]
pub(crate) mod peer_connection_test;

use crate::api::media_engine::MediaEngine;
use crate::api::setting_engine::SettingEngine;
use crate::api::API;
use crate::data::data_channel::DataChannel;
use crate::data::sctp_transport::SCTPTransport;
use crate::media::dtls_transport::dtls_transport_state::DTLSTransportState;
use crate::media::dtls_transport::DTLSTransport;
use crate::media::ice_transport::ice_transport_state::ICETransportState;
use crate::media::ice_transport::ICETransport;
use crate::media::interceptor::{Attributes, Interceptor, RTCPWriter};
use crate::media::rtp::rtp_receiver::RTPReceiver;
use crate::media::rtp::rtp_transceiver::{
    find_by_mid, handle_unknown_rtp_packet, satisfy_type_and_direction, RTPTransceiver,
};
use crate::media::track::track_remote::TrackRemote;
use crate::peer::configuration::Configuration;
use crate::peer::ice::ice_connection_state::ICEConnectionState;
use crate::peer::ice::ice_gather::ice_gatherer::{
    ICEGatherer, OnGatheringCompleteHdlrFn, OnICEGathererStateChangeHdlrFn, OnLocalCandidateHdlrFn,
};
use crate::peer::ice::ice_gather::ICEGatherOptions;
use crate::peer::peer_connection_state::{NegotiationNeededState, PeerConnectionState};
use crate::peer::policy::bundle_policy::BundlePolicy;
use crate::peer::policy::ice_transport_policy::ICETransportPolicy;
use crate::peer::policy::rtcp_mux_policy::RTCPMuxPolicy;
use crate::peer::policy::sdp_semantics::SDPSemantics;
use crate::peer::sdp::session_description::{SessionDescription, SessionDescriptionSerde};
use crate::peer::signaling_state::{check_next_signaling_state, SignalingState, StateChangeOp};

use crate::data::data_channel::data_channel_config::DataChannelConfig;
use crate::data::data_channel::data_channel_parameters::DataChannelParameters;
use crate::data::data_channel::data_channel_state::DataChannelState;
use crate::data::sctp_transport::sctp_transport_capabilities::SCTPTransportCapabilities;
use crate::data::sctp_transport::sctp_transport_state::SCTPTransportState;
use crate::error::Error;
//use crate::media::dtls_transport::dtls_fingerprint::DTLSFingerprint;
//use crate::media::dtls_transport::dtls_parameters::DTLSParameters;
use crate::media::dtls_transport::dtls_fingerprint::DTLSFingerprint;
use crate::media::dtls_transport::dtls_parameters::DTLSParameters;
use crate::media::dtls_transport::dtls_role::{
    DTLSRole, DEFAULT_DTLS_ROLE_ANSWER, DEFAULT_DTLS_ROLE_OFFER,
};
use crate::media::rtp::rtp_codec::{RTPCodecType, RTPHeaderExtensionCapability};
use crate::media::rtp::rtp_sender::RTPSender;
use crate::media::rtp::rtp_transceiver_direction::RTPTransceiverDirection;
use crate::media::rtp::{RTPCodingParameters, RTPReceiveParameters, RTPTransceiverInit, SSRC};
use crate::media::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use crate::media::track::track_local::TrackLocal;
use crate::peer::ice::ice_candidate::{ICECandidate, ICECandidateInit};
use crate::peer::ice::ice_gather::ice_gatherer_state::ICEGathererState;
use crate::peer::ice::ice_gather::ice_gathering_state::ICEGatheringState;
use crate::peer::ice::ice_role::ICERole;
use crate::peer::ice::ICEParameters;
use crate::peer::offer_answer_options::{AnswerOptions, OfferOptions};
use crate::peer::operation::Operations;
use crate::peer::sdp::sdp_type::SDPType;
use crate::peer::sdp::*;
use crate::util::{flatten_errs, math_rand_alpha};
use crate::{
    MEDIA_SECTION_APPLICATION, RECEIVE_MTU, SIMULCAST_MAX_PROBE_ROUTINES, SIMULCAST_PROBE_COUNT,
    SSRC_STR,
};
use anyhow::Result;
use defer::defer;
use ice::candidate::candidate_base::unmarshal_candidate;
use ice::candidate::Candidate;
use sdp::session_description::{ATTR_KEY_ICELITE, ATTR_KEY_MSID};
use sdp::util::ConnectionRole;
use std::future::Future;
use std::io::Read;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex};

pub type OnSignalingStateChangeHdlrFn = Box<
    dyn (FnMut(SignalingState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync,
>;

pub type OnICEConnectionStateChangeHdlrFn = Box<
    dyn (FnMut(ICEConnectionState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnPeerConnectionStateChangeHdlrFn = Box<
    dyn (FnMut(PeerConnectionState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnDataChannelHdlrFn = Box<
    dyn (FnMut(Arc<DataChannel>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnTrackHdlrFn = Box<
    dyn (FnMut(
            Option<Arc<TrackRemote>>,
            Option<Arc<RTPReceiver>>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnNegotiationNeededHdlrFn =
    Box<dyn (FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

struct StartTransportsParams {
    ice_transport: Arc<ICETransport>,
    dtls_transport: Arc<DTLSTransport>,
    on_peer_connection_state_change_handler: Arc<Mutex<Option<OnPeerConnectionStateChangeHdlrFn>>>,
    is_closed: Arc<AtomicBool>,
    peer_connection_state: Arc<AtomicU8>,
    ice_connection_state: Arc<AtomicU8>,
}

/// PeerConnection represents a WebRTC connection that establishes a
/// peer-to-peer communications with another PeerConnection instance in a
/// browser, or to another endpoint implementing the required protocols.
#[derive(Default)]
pub struct PeerConnection {
    stats_id: String,

    sdp_origin: sdp::session_description::Origin,

    // ops is an operations queue which will ensure the enqueued actions are
    // executed in order. It is used for asynchronously, but serially processing
    // remote and local descriptions
    ops: Operations,

    configuration: Configuration,

    current_local_description: Option<SessionDescription>,
    pending_local_description: Option<SessionDescription>,
    current_remote_description: Option<SessionDescription>,
    pending_remote_description: Option<SessionDescription>,
    signaling_state: AtomicU8,            //SignalingState,
    ice_connection_state: Arc<AtomicU8>,  //ICEConnectionState,
    peer_connection_state: Arc<AtomicU8>, //PeerConnectionState,

    idp_login_url: Option<String>,

    is_closed: Arc<AtomicBool>,         //*atomicBool
    is_negotiation_needed: AtomicBool,  //*atomicBool
    negotiation_needed_state: AtomicU8, //NegotiationNeededState,

    last_offer: String,
    last_answer: String,

    /// a value containing the last known greater mid value
    /// we internally generate mids as numbers. Needed since JSEP
    /// requires that when reusing a media section a new unique mid
    /// should be defined (see JSEP 3.4.1).
    greater_mid: isize,

    rtp_transceivers: Vec<Arc<RTPTransceiver>>,

    on_track_handler: Arc<Mutex<Option<OnTrackHdlrFn>>>,
    on_signaling_state_change_handler: Arc<Mutex<Option<OnSignalingStateChangeHdlrFn>>>,
    on_peer_connection_state_change_handler: Arc<Mutex<Option<OnPeerConnectionStateChangeHdlrFn>>>,
    on_ice_connection_state_change_handler: Arc<Mutex<Option<OnICEConnectionStateChangeHdlrFn>>>,
    on_data_channel_handler: Arc<Mutex<Option<OnDataChannelHdlrFn>>>,
    on_negotiation_needed_handler: Arc<Mutex<Option<OnNegotiationNeededHdlrFn>>>,

    // interceptor_rtcpwriter interceptor.RTCPWriter
    ice_gatherer: Arc<ICEGatherer>,
    ice_transport: Arc<ICETransport>,
    dtls_transport: Arc<DTLSTransport>,
    sctp_transport: Arc<SCTPTransport>,

    // A reference to the associated API state used by this connection
    setting_engine: Arc<SettingEngine>,
    pub(crate) media_engine: Arc<MediaEngine>,
    interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,

    interceptor_rtcp_writer: Option<Arc<dyn RTCPWriter + Send + Sync>>,
}

impl PeerConnection {
    /// creates a PeerConnection with the default codecs and
    /// interceptors.  See register_default_codecs and RegisterDefaultInterceptors.
    ///
    /// If you wish to customize the set of available codecs or the set of
    /// active interceptors, create a MediaEngine and call api.new_peer_connection
    /// instead of this function.
    pub(crate) async fn new(api: &API, configuration: Configuration) -> Result<Self> {
        // https://w3c.github.io/webrtc-pc/#constructor (Step #2)
        // Some variables defined explicitly despite their implicit zero values to
        // allow better readability to understand what is happening.
        let mut pc = PeerConnection {
            stats_id: format!(
                "PeerConnection-{}",
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ),
            configuration: Configuration {
                ice_servers: vec![],
                ice_transport_policy: ICETransportPolicy::All,
                bundle_policy: BundlePolicy::Balanced,
                rtcp_mux_policy: RTCPMuxPolicy::Require,
                peer_identity: String::new(),
                certificates: vec![],
                ice_candidate_pool_size: 0,
                sdp_semantics: SDPSemantics::default(),
            },
            ops: Operations::new(),
            is_closed: Arc::new(AtomicBool::new(false)),
            is_negotiation_needed: AtomicBool::new(false),
            negotiation_needed_state: AtomicU8::new(NegotiationNeededState::Empty as u8),
            last_offer: "".to_owned(),
            last_answer: "".to_owned(),
            greater_mid: -1,
            signaling_state: AtomicU8::new(SignalingState::Stable as u8),
            ice_connection_state: Arc::new(AtomicU8::new(ICEConnectionState::New as u8)),
            peer_connection_state: Arc::new(AtomicU8::new(PeerConnectionState::New as u8)),

            setting_engine: Arc::clone(&api.setting_engine),
            media_engine: if !api.setting_engine.disable_media_engine_copy {
                Arc::new(api.media_engine.clone_to())
            } else {
                Arc::clone(&api.media_engine)
            },
            interceptor: api.interceptor.clone(),

            ..Default::default()
        };

        pc.init_configuration(configuration)?;

        // Create the ice gatherer
        pc.ice_gatherer = Arc::new(api.new_ice_gatherer(ICEGatherOptions {
            ice_servers: pc.configuration.get_ice_servers(),
            ice_gather_policy: pc.configuration.ice_transport_policy,
        })?);

        // Part I of Create the ice transport
        pc.ice_transport = Arc::new(api.new_ice_transport(Arc::clone(&pc.ice_gatherer)));

        // Create the DTLS transport
        pc.dtls_transport = Arc::new(api.new_dtls_transport(
            Arc::clone(&pc.ice_transport),
            pc.configuration.certificates.clone(),
        )?);

        // Create the SCTP transport
        pc.sctp_transport = Arc::new(api.new_sctp_transport(Arc::clone(&pc.dtls_transport))?);

        // Wire up the on datachannel handler
        let on_data_channel_handler = Arc::clone(&pc.on_data_channel_handler);
        pc.sctp_transport
            .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
                let on_data_channel_handler2 = Arc::clone(&on_data_channel_handler);
                Box::pin(async move {
                    let mut handler = on_data_channel_handler2.lock().await;
                    if let Some(f) = &mut *handler {
                        f(d).await;
                    }
                })
            }))
            .await;

        // Part II of Create the ice transport
        pc.setup_ice_transport().await;

        //TODO: pc.interceptor_rtcp_writer = api.interceptor.bind_rtcp_writer(interceptor.RTCPWriterFunc(pc.writeRTCP))

        Ok(pc)
    }

    /// init_configuration defines validation of the specified Configuration and
    /// its assignment to the internal configuration variable. This function differs
    /// from its set_configuration counterpart because most of the checks do not
    /// include verification statements related to the existing state. Thus the
    /// function describes only minor verification of some the struct variables.
    fn init_configuration(&mut self, configuration: Configuration) -> Result<()> {
        let sanitized_ice_servers = configuration.get_ice_servers();
        if !sanitized_ice_servers.is_empty() {
            for server in &sanitized_ice_servers {
                server.validate()?;
            }
            self.configuration.ice_servers = sanitized_ice_servers;
        }

        if !configuration.peer_identity.is_empty() {
            self.configuration.peer_identity = configuration.peer_identity;
        }

        // https://www.w3.org/TR/webrtc/#constructor (step #3)
        /*TODO:if !configuration.certificates.is_empty() {
            now := time.Now()
            for _, x509Cert := range configuration.Certificates {
                if !x509Cert.Expires().IsZero() && now.After(x509Cert.Expires()) {
                    return &rtcerr.InvalidAccessError{Err: ErrCertificateExpired}
                }
                pc.configuration.Certificates = append(pc.configuration.Certificates, x509Cert)
            }
        } else {
            sk, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
            if err != nil {
                return &rtcerr.UnknownError{Err: err}
            }
            certificate, err := GenerateCertificate(sk)
            if err != nil {
                return err
            }
            pc.configuration.Certificates = []Certificate{*certificate}
        }  */

        if configuration.bundle_policy != BundlePolicy::Unspecified {
            self.configuration.bundle_policy = configuration.bundle_policy;
        }

        if configuration.rtcp_mux_policy != RTCPMuxPolicy::Unspecified {
            self.configuration.rtcp_mux_policy = configuration.rtcp_mux_policy;
        }

        if configuration.ice_candidate_pool_size != 0 {
            self.configuration.ice_candidate_pool_size = configuration.ice_candidate_pool_size;
        }

        if configuration.ice_transport_policy != ICETransportPolicy::Unspecified {
            self.configuration.ice_transport_policy = configuration.ice_transport_policy;
        }

        if configuration.sdp_semantics != SDPSemantics::Unspecified {
            self.configuration.sdp_semantics = configuration.sdp_semantics;
        }

        Ok(())
    }

    /// on_signaling_state_change sets an event handler which is invoked when the
    /// peer connection's signaling state changes
    pub async fn on_signaling_state_change(&self, f: OnSignalingStateChangeHdlrFn) {
        let mut on_signaling_state_change_handler =
            self.on_signaling_state_change_handler.lock().await;
        *on_signaling_state_change_handler = Some(f);
    }

    async fn do_signaling_state_change(&self, new_state: SignalingState) {
        log::info!("signaling state changed to {}", new_state);
        let mut handler = self.on_signaling_state_change_handler.lock().await;
        if let Some(f) = &mut *handler {
            f(new_state).await;
        }
    }

    /// on_data_channel sets an event handler which is invoked when a data
    /// channel message arrives from a remote peer.
    pub async fn on_data_channel(&self, f: OnDataChannelHdlrFn) {
        let mut on_data_channel_handler = self.on_data_channel_handler.lock().await;
        *on_data_channel_handler = Some(f);
    }

    /// on_negotiation_needed sets an event handler which is invoked when
    /// a change has occurred which requires session negotiation
    pub async fn on_negotiation_needed(&self, f: OnNegotiationNeededHdlrFn) {
        let mut on_negotiation_needed_handler = self.on_negotiation_needed_handler.lock().await;
        *on_negotiation_needed_handler = Some(f);
    }

    /// do_negotiation_needed enqueues negotiation_needed_op if necessary
    /// caller of this method should hold `pc.mu` lock
    async fn do_negotiation_needed(&self) {
        // https://w3c.github.io/webrtc-pc/#updating-the-negotiation-needed-flag
        // non-canon step 1
        let negotiation_needed_state: NegotiationNeededState =
            self.negotiation_needed_state.load(Ordering::SeqCst).into();
        if negotiation_needed_state == NegotiationNeededState::Run {
            self.negotiation_needed_state
                .store(NegotiationNeededState::Queue as u8, Ordering::SeqCst);
            return;
        } else if negotiation_needed_state == NegotiationNeededState::Queue {
            return;
        }
        self.negotiation_needed_state
            .store(NegotiationNeededState::Run as u8, Ordering::SeqCst);
        //TODO: pc.ops.Enqueue(pc.negotiation_needed_op)
        /*let _ = self
        .ops
        .enqueue(Operation(Box::new(move || Box::pin(async {}))))
        .await;*/
    }

    async fn negotiation_needed_op(&self) {
        // Don't run NegotiatedNeeded checks if on_negotiation_needed is not set
        {
            let handler = self.on_negotiation_needed_handler.lock().await;
            if handler.is_none() {
                return;
            }
        }

        // https://www.w3.org/TR/webrtc/#updating-the-negotiation-needed-flag
        // Step 2.1
        if self.is_closed.load(Ordering::SeqCst) {
            return;
        }
        // non-canon step 2.2
        if !self.ops.is_empty().await {
            //TODO: pc.ops.Enqueue(pc.negotiation_needed_op)
            return;
        }

        // non-canon, run again if there was a request
        defer(|| {
            if self.negotiation_needed_state.load(Ordering::SeqCst)
                == NegotiationNeededState::Queue as u8
            {
                Box::pin(async {
                    self.do_negotiation_needed().await;
                });
            } else {
                self.negotiation_needed_state
                    .store(NegotiationNeededState::Empty as u8, Ordering::SeqCst);
            }
        });

        // Step 2.3
        if self.signaling_state() != SignalingState::Stable {
            return;
        }

        // Step 2.4
        if !self.check_negotiation_needed().await {
            self.is_negotiation_needed.store(false, Ordering::SeqCst);
            return;
        }

        // Step 2.5
        if self.is_negotiation_needed.load(Ordering::SeqCst) {
            return;
        }

        // Step 2.6
        self.is_negotiation_needed.store(true, Ordering::SeqCst);

        // Step 2.7
        let mut handler = self.on_negotiation_needed_handler.lock().await;
        if let Some(f) = &mut *handler {
            f().await;
        }
    }

    async fn check_negotiation_needed(&self) -> bool {
        // To check if negotiation is needed for connection, perform the following checks:
        // Skip 1, 2 steps
        // Step 3
        if let Some(local_desc) = &self.current_local_description {
            let len_data_channel = {
                let data_channels = self.sctp_transport.data_channels.lock().await;
                data_channels.len()
            };

            if len_data_channel != 0 && have_data_channel(local_desc).is_none() {
                return true;
            }

            for t in &self.rtp_transceivers {
                // https://www.w3.org/TR/webrtc/#dfn-update-the-negotiation-needed-flag
                // Step 5.1
                // if t.stopping && !t.stopped {
                // 	return true
                // }
                let m = get_by_mid(t.mid().await.as_str(), local_desc);
                // Step 5.2
                if !t.stopped && m.is_none() {
                    return true;
                }
                if !t.stopped {
                    if let Some(m) = m {
                        // Step 5.3.1
                        if t.direction() == RTPTransceiverDirection::Sendrecv
                            || t.direction() == RTPTransceiverDirection::Sendonly
                        {
                            if let (Some(desc_msid), Some(sender)) =
                                (m.attribute(ATTR_KEY_MSID), t.sender().await)
                            {
                                if let Some(track) = &sender.track().await {
                                    if desc_msid.as_str()
                                        != track.stream_id().to_owned() + " " + track.id()
                                    {
                                        return true;
                                    }
                                }
                            } else {
                                return true;
                            }
                        }
                        match local_desc.serde.sdp_type {
                            SDPType::Offer => {
                                // Step 5.3.2
                                if let Some(remote_desc) = &self.current_remote_description {
                                    if let Some(rm) =
                                        get_by_mid(t.mid().await.as_str(), remote_desc)
                                    {
                                        if get_peer_direction(m) != t.direction()
                                            && get_peer_direction(rm) != t.direction().reverse()
                                        {
                                            return true;
                                        }
                                    } else {
                                        return true;
                                    }
                                }
                            }
                            SDPType::Answer => {
                                // Step 5.3.3
                                if m.attribute(t.direction().to_string().as_str()).is_none() {
                                    return true;
                                }
                            }
                            _ => {}
                        };
                    }
                }
                // Step 5.4
                if t.stopped && !t.mid().await.is_empty() {
                    if let Some(remote_desc) = &self.current_remote_description {
                        if get_by_mid(t.mid().await.as_str(), local_desc).is_some()
                            || get_by_mid(t.mid().await.as_str(), remote_desc).is_some()
                        {
                            return true;
                        }
                    }
                }
            }
            // Step 6
            false
        } else {
            true
        }
    }

    /// on_ice_candidate sets an event handler which is invoked when a new ICE
    /// candidate is found.
    /// Take note that the handler is gonna be called with a nil pointer when
    /// gathering is finished.
    pub async fn on_ice_candidate(&self, f: OnLocalCandidateHdlrFn) {
        self.ice_gatherer.on_local_candidate(f).await
    }

    /// on_ice_gathering_state_change sets an event handler which is invoked when the
    /// ICE candidate gathering state has changed.
    pub async fn on_ice_gathering_state_change(&self, f: OnICEGathererStateChangeHdlrFn) {
        self.ice_gatherer.on_state_change(f).await
    }

    /// on_track sets an event handler which is called when remote track
    /// arrives from a remote peer.
    pub async fn on_track(&self, f: OnTrackHdlrFn) {
        let mut on_track_handler = self.on_track_handler.lock().await;
        *on_track_handler = Some(f);
    }

    async fn do_track(&self, t: Option<Arc<TrackRemote>>, r: Option<Arc<RTPReceiver>>) {
        log::debug!(
            "got new track: {}",
            if let Some(t) = &t {
                t.id().await
            } else {
                "None".to_owned()
            }
        );

        if t.is_some() {
            let mut handler = self.on_track_handler.lock().await;
            if let Some(f) = &mut *handler {
                f(t, r).await;
            } else {
                log::warn!("on_track unset, unable to handle incoming media streams");
            }
        }
    }

    /// on_ice_connection_state_change sets an event handler which is called
    /// when an ICE connection state is changed.
    pub async fn on_ice_connection_state_change(&self, f: OnICEConnectionStateChangeHdlrFn) {
        let mut on_ice_connection_state_change_handler =
            self.on_ice_connection_state_change_handler.lock().await;
        *on_ice_connection_state_change_handler = Some(f);
    }

    async fn do_ice_connection_state_change(
        on_ice_connection_state_change_handler: &Arc<
            Mutex<Option<OnICEConnectionStateChangeHdlrFn>>,
        >,
        ice_connection_state: &Arc<AtomicU8>,
        cs: ICEConnectionState,
    ) {
        ice_connection_state.store(cs as u8, Ordering::SeqCst);

        log::info!("ICE connection state changed: {}", cs);
        let mut handler = on_ice_connection_state_change_handler.lock().await;
        if let Some(f) = &mut *handler {
            f(cs).await;
        }
    }

    /// on_peer_connection_state_change sets an event handler which is called
    /// when the PeerConnectionState has changed
    pub async fn on_peer_connection_state_change(&self, f: OnPeerConnectionStateChangeHdlrFn) {
        let mut on_peer_connection_state_change_handler =
            self.on_peer_connection_state_change_handler.lock().await;
        *on_peer_connection_state_change_handler = Some(f);
    }

    async fn do_peer_connection_state_change(
        on_peer_connection_state_change_handler: &Arc<
            Mutex<Option<OnPeerConnectionStateChangeHdlrFn>>,
        >,
        cs: PeerConnectionState,
    ) {
        log::info!("Peer connection state changed: {}", cs);
        let mut handler = on_peer_connection_state_change_handler.lock().await;
        if let Some(f) = &mut *handler {
            f(cs).await;
        }
    }

    /// set_configuration updates the configuration of this PeerConnection object.
    pub async fn set_configuration(&mut self, configuration: Configuration) -> Result<()> {
        //nolint:gocognit
        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-setconfiguration (step #2)
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed.into());
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #3)
        if !configuration.peer_identity.is_empty() {
            if configuration.peer_identity != self.configuration.peer_identity {
                return Err(Error::ErrModifyingPeerIdentity.into());
            }
            self.configuration.peer_identity = configuration.peer_identity;
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #4)
        if !configuration.certificates.is_empty() {
            if configuration.certificates.len() != self.configuration.certificates.len() {
                return Err(Error::ErrModifyingCertificates.into());
            }

            /*TODO: for (i, certificate) in configuration.certificates.iter().enumerate() {
                if !self.configuration.certificates[i].Equals(certificate) {
                    return Err(Error::ErrModifyingCertificates.into());
                }
            }*/
            self.configuration.certificates = configuration.certificates;
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #5)
        if configuration.bundle_policy != BundlePolicy::Unspecified {
            if configuration.bundle_policy != self.configuration.bundle_policy {
                return Err(Error::ErrModifyingBundlePolicy.into());
            }
            self.configuration.bundle_policy = configuration.bundle_policy;
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #6)
        if configuration.rtcp_mux_policy != RTCPMuxPolicy::Unspecified {
            if configuration.rtcp_mux_policy != self.configuration.rtcp_mux_policy {
                return Err(Error::ErrModifyingRTCPMuxPolicy.into());
            }
            self.configuration.rtcp_mux_policy = configuration.rtcp_mux_policy;
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #7)
        if configuration.ice_candidate_pool_size != 0 {
            if self.configuration.ice_candidate_pool_size != configuration.ice_candidate_pool_size
                && self.local_description().await.is_some()
            {
                return Err(Error::ErrModifyingICECandidatePoolSize.into());
            }
            self.configuration.ice_candidate_pool_size = configuration.ice_candidate_pool_size;
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #8)
        if configuration.ice_transport_policy != ICETransportPolicy::Unspecified {
            self.configuration.ice_transport_policy = configuration.ice_transport_policy
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #11)
        if !configuration.ice_servers.is_empty() {
            // https://www.w3.org/TR/webrtc/#set-the-configuration (step #11.3)
            for server in &configuration.ice_servers {
                server.validate()?;
            }
            self.configuration.ice_servers = configuration.ice_servers
        }
        Ok(())
    }

    /// get_configuration returns a Configuration object representing the current
    /// configuration of this PeerConnection object. The returned object is a
    /// copy and direct mutation on it will not take affect until set_configuration
    /// has been called with Configuration passed as its only argument.
    /// https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-getconfiguration
    pub fn get_configuration(&self) -> &Configuration {
        &self.configuration
    }

    fn get_stats_id(&self) -> &str {
        self.stats_id.as_str()
    }

    /// has_local_description_changed returns whether local media (rtp_transceivers) has changed
    /// caller of this method should hold `pc.mu` lock
    async fn has_local_description_changed(&self, desc: &SessionDescription) -> bool {
        for t in &self.rtp_transceivers {
            if let Some(m) = get_by_mid(t.mid().await.as_str(), desc) {
                if get_peer_direction(m) != t.direction() {
                    return true;
                }
            } else {
                return true;
            }
        }
        false
    }

    /// create_offer starts the PeerConnection and generates the localDescription
    /// https://w3c.github.io/webrtc-pc/#dom-rtcpeerconnection-createoffer
    pub async fn create_offer(
        &mut self,
        options: Option<OfferOptions>,
    ) -> Result<SessionDescription> {
        let use_identity = self.idp_login_url.is_some();
        if use_identity {
            return Err(Error::ErrIdentityProviderNotImplemented.into());
        } else if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed.into());
        }

        if let Some(options) = options {
            if options.ice_restart {
                self.ice_transport.restart().await?;
            }
        }

        // This may be necessary to recompute if, for example, createOffer was called when only an
        // audio RTCRtpTransceiver was added to connection, but while performing the in-parallel
        // steps to create an offer, a video RTCRtpTransceiver was added, requiring additional
        // inspection of video system resources.
        let mut count = 0;
        let mut offer;

        loop {
            // We cache current transceivers to ensure they aren't
            // mutated during offer generation. We later check if they have
            // been mutated and recompute the offer if necessary.
            let current_transceivers = &mut self.rtp_transceivers;

            // in-parallel steps to create an offer
            // https://w3c.github.io/webrtc-pc/#dfn-in-parallel-steps-to-create-an-offer
            let is_plan_b = if self.current_remote_description.is_some() {
                description_is_plan_b(self.current_remote_description.as_ref())?
            } else {
                self.configuration.sdp_semantics == SDPSemantics::PlanB
            };

            // include unmatched local transceivers
            if !is_plan_b {
                // update the greater mid if the remote description provides a greater one
                if let Some(current_remote_description) = &self.current_remote_description {
                    if let Some(parsed) = &current_remote_description.parsed {
                        for media in &parsed.media_descriptions {
                            if let Some(mid) = get_mid_value(media) {
                                if mid.is_empty() {
                                    continue;
                                }
                                let numeric_mid = match mid.parse::<isize>() {
                                    Ok(n) => n,
                                    Err(_) => continue,
                                };
                                if numeric_mid > self.greater_mid {
                                    self.greater_mid = numeric_mid;
                                }
                            }
                        }
                    }
                }
                for t in current_transceivers {
                    if !t.mid().await.is_empty() {
                        continue;
                    }
                    self.greater_mid += 1;
                    t.set_mid(format!("{}", self.greater_mid)).await?;
                }
            }

            let mut d = if self.current_remote_description.is_none() {
                self.generate_unmatched_sdp(/*current_transceivers,*/ use_identity)
                    .await?
            } else {
                self.generate_matched_sdp(
                    /*current_transceivers,*/
                    use_identity,
                    true, /*includeUnmatched */
                    DEFAULT_DTLS_ROLE_OFFER.to_connection_role(),
                )
                .await?
            };

            update_sdp_origin(&mut self.sdp_origin, &mut d);
            let sdp = d.marshal();

            offer = SessionDescription {
                serde: SessionDescriptionSerde {
                    sdp_type: SDPType::Offer,
                    sdp,
                },
                parsed: Some(d),
            };

            // Verify local media hasn't changed during offer
            // generation. Recompute if necessary
            if is_plan_b || !self.has_local_description_changed(&offer).await {
                break;
            }
            count += 1;
            if count >= 128 {
                return Err(Error::ErrExcessiveRetries.into());
            }
        }

        self.last_offer = offer.serde.sdp.clone();
        Ok(offer)
    }

    /// Update the PeerConnectionState given the state of relevant transports
    /// https://www.w3.org/TR/webrtc/#rtcpeerconnectionstate-enum
    async fn update_connection_state(
        on_peer_connection_state_change_handler: &Arc<
            Mutex<Option<OnPeerConnectionStateChangeHdlrFn>>,
        >,
        is_closed: &Arc<AtomicBool>,
        peer_connection_state: &Arc<AtomicU8>,
        ice_connection_state: ICEConnectionState,
        dtls_transport_state: DTLSTransportState,
    ) {
        let  connection_state =
        // The RTCPeerConnection object's [[IsClosed]] slot is true.
        if is_closed.load(Ordering::SeqCst) {
             PeerConnectionState::Closed
        }else if ice_connection_state == ICEConnectionState::Failed || dtls_transport_state == DTLSTransportState::Failed {
            // Any of the RTCIceTransports or RTCDtlsTransports are in a "failed" state.
             PeerConnectionState::Failed
        }else if ice_connection_state == ICEConnectionState::Disconnected {
            // Any of the RTCIceTransports or RTCDtlsTransports are in the "disconnected"
            // state and none of them are in the "failed" or "connecting" or "checking" state.
            PeerConnectionState::Disconnected
        }else if ice_connection_state == ICEConnectionState::Connected && dtls_transport_state == DTLSTransportState::Connected {
            // All RTCIceTransports and RTCDtlsTransports are in the "connected", "completed" or "closed"
            // state and at least one of them is in the "connected" or "completed" state.
            PeerConnectionState::Connected
        }else if ice_connection_state == ICEConnectionState::Checking && dtls_transport_state == DTLSTransportState::Connecting{
        //  Any of the RTCIceTransports or RTCDtlsTransports are in the "connecting" or
        // "checking" state and none of them is in the "failed" state.
             PeerConnectionState::Connecting
        }else{
            PeerConnectionState::New
        };

        if peer_connection_state.load(Ordering::SeqCst) == connection_state as u8 {
            return;
        }

        log::info!("peer connection state changed: {}", connection_state);
        peer_connection_state.store(connection_state as u8, Ordering::SeqCst);

        PeerConnection::do_peer_connection_state_change(
            on_peer_connection_state_change_handler,
            connection_state,
        )
        .await;
    }

    async fn setup_ice_transport(&self) {
        let ice_connection_state = Arc::clone(&self.ice_connection_state);
        let peer_connection_state = Arc::clone(&self.peer_connection_state);
        let is_closed = Arc::clone(&self.is_closed);
        let dtls_transport = Arc::clone(&self.dtls_transport);
        let on_ice_connection_state_change_handler =
            Arc::clone(&self.on_ice_connection_state_change_handler);
        let on_peer_connection_state_change_handler =
            Arc::clone(&self.on_peer_connection_state_change_handler);

        self.ice_transport
            .on_connection_state_change(Box::new(move |state: ICETransportState| {
                let cs = match state {
                    ICETransportState::New => ICEConnectionState::New,
                    ICETransportState::Checking => ICEConnectionState::Checking,
                    ICETransportState::Connected => ICEConnectionState::Connected,
                    ICETransportState::Completed => ICEConnectionState::Completed,
                    ICETransportState::Failed => ICEConnectionState::Failed,
                    ICETransportState::Disconnected => ICEConnectionState::Disconnected,
                    ICETransportState::Closed => ICEConnectionState::Closed,
                    _ => {
                        log::warn!("on_connection_state_change: unhandled ICE state: {}", state);
                        return Box::pin(async {});
                    }
                };

                let ice_connection_state2 = Arc::clone(&ice_connection_state);
                let on_ice_connection_state_change_handler2 =
                    Arc::clone(&on_ice_connection_state_change_handler);
                let on_peer_connection_state_change_handler2 =
                    Arc::clone(&on_peer_connection_state_change_handler);
                let is_closed2 = Arc::clone(&is_closed);
                let dtls_transport_state = dtls_transport.state();
                let peer_connection_state2 = Arc::clone(&peer_connection_state);
                Box::pin(async move {
                    PeerConnection::do_ice_connection_state_change(
                        &on_ice_connection_state_change_handler2,
                        &ice_connection_state2,
                        cs,
                    )
                    .await;

                    PeerConnection::update_connection_state(
                        &on_peer_connection_state_change_handler2,
                        &is_closed2,
                        &peer_connection_state2,
                        cs,
                        dtls_transport_state,
                    )
                    .await;
                })
            }))
            .await;
    }

    /// create_answer starts the PeerConnection and generates the localDescription
    pub async fn create_answer(
        &mut self,
        _options: Option<AnswerOptions>,
    ) -> Result<SessionDescription> {
        let use_identity = self.idp_login_url.is_some();
        if self.remote_description().is_none() {
            return Err(Error::ErrNoRemoteDescription.into());
        } else if use_identity {
            return Err(Error::ErrIdentityProviderNotImplemented.into());
        } else if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed.into());
        } else if self.signaling_state() != SignalingState::HaveRemoteOffer
            && self.signaling_state() != SignalingState::HaveLocalPranswer
        {
            return Err(Error::ErrIncorrectSignalingState.into());
        }

        let mut connection_role = self.setting_engine.answering_dtls_role.to_connection_role();
        if connection_role == ConnectionRole::Unspecified {
            connection_role = DEFAULT_DTLS_ROLE_ANSWER.to_connection_role();
        }

        let mut d = self
            .generate_matched_sdp(
                /*self.rtp_transceivers,*/ use_identity,
                false, /*includeUnmatched */
                connection_role,
            )
            .await?;

        update_sdp_origin(&mut self.sdp_origin, &mut d);
        let sdp = d.marshal();

        let answer = SessionDescription {
            serde: SessionDescriptionSerde {
                sdp_type: SDPType::Answer,
                sdp,
            },
            parsed: Some(d),
        };

        self.last_answer = answer.serde.sdp.clone();
        Ok(answer)
    }

    // 4.4.1.6 Set the SessionDescription
    pub(crate) async fn set_description(
        &mut self,
        sd: &SessionDescription,
        op: StateChangeOp,
    ) -> Result<()> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed.into());
        } else if sd.serde.sdp_type == SDPType::Unspecified {
            return Err(Error::ErrPeerConnSDPTypeInvalidValue.into());
        }

        let next_state = {
            let cur = self.signaling_state();
            let new_sdpdoes_not_match_offer = Error::ErrSDPDoesNotMatchOffer;
            let new_sdpdoes_not_match_answer = Error::ErrSDPDoesNotMatchAnswer;

            match op {
                StateChangeOp::SetLocal => {
                    match sd.serde.sdp_type {
                        // stable->SetLocal(offer)->have-local-offer
                        SDPType::Offer => {
                            if sd.serde.sdp != self.last_offer {
                                Err(new_sdpdoes_not_match_offer.into())
                            } else {
                                let next_state = check_next_signaling_state(
                                    cur,
                                    SignalingState::HaveLocalOffer,
                                    StateChangeOp::SetLocal,
                                    sd.serde.sdp_type,
                                );
                                if next_state.is_ok() {
                                    self.pending_local_description = Some(sd.clone());
                                }
                                next_state
                            }
                        }
                        // have-remote-offer->SetLocal(answer)->stable
                        // have-local-pranswer->SetLocal(answer)->stable
                        SDPType::Answer => {
                            if sd.serde.sdp != self.last_answer {
                                Err(new_sdpdoes_not_match_answer.into())
                            } else {
                                let next_state = check_next_signaling_state(
                                    cur,
                                    SignalingState::Stable,
                                    StateChangeOp::SetLocal,
                                    sd.serde.sdp_type,
                                );
                                if next_state.is_ok() {
                                    self.current_local_description = Some(sd.clone());
                                    self.current_remote_description =
                                        self.pending_remote_description.take();
                                    self.pending_remote_description = None;
                                    self.pending_local_description = None;
                                }
                                next_state
                            }
                        }
                        SDPType::Rollback => {
                            let next_state = check_next_signaling_state(
                                cur,
                                SignalingState::Stable,
                                StateChangeOp::SetLocal,
                                sd.serde.sdp_type,
                            );
                            if next_state.is_ok() {
                                self.pending_local_description = None;
                            }
                            next_state
                        }
                        // have-remote-offer->SetLocal(pranswer)->have-local-pranswer
                        SDPType::Pranswer => {
                            if sd.serde.sdp != self.last_answer {
                                Err(new_sdpdoes_not_match_answer.into())
                            } else {
                                let next_state = check_next_signaling_state(
                                    cur,
                                    SignalingState::HaveLocalPranswer,
                                    StateChangeOp::SetLocal,
                                    sd.serde.sdp_type,
                                );
                                if next_state.is_ok() {
                                    self.pending_local_description = Some(sd.clone());
                                }
                                next_state
                            }
                        }
                        _ => Err(Error::ErrPeerConnStateChangeInvalid.into()),
                    }
                }
                StateChangeOp::SetRemote => {
                    match sd.serde.sdp_type {
                        // stable->SetRemote(offer)->have-remote-offer
                        SDPType::Offer => {
                            let next_state = check_next_signaling_state(
                                cur,
                                SignalingState::HaveRemoteOffer,
                                StateChangeOp::SetRemote,
                                sd.serde.sdp_type,
                            );
                            if next_state.is_ok() {
                                self.pending_remote_description = Some(sd.clone());
                            }
                            next_state
                        }
                        // have-local-offer->SetRemote(answer)->stable
                        // have-remote-pranswer->SetRemote(answer)->stable
                        SDPType::Answer => {
                            let next_state = check_next_signaling_state(
                                cur,
                                SignalingState::Stable,
                                StateChangeOp::SetRemote,
                                sd.serde.sdp_type,
                            );
                            if next_state.is_ok() {
                                self.current_remote_description = Some(sd.clone());
                                self.current_local_description =
                                    self.pending_local_description.take();
                                self.pending_remote_description = None;
                                self.pending_local_description = None;
                            }
                            next_state
                        }
                        SDPType::Rollback => {
                            let next_state = check_next_signaling_state(
                                cur,
                                SignalingState::Stable,
                                StateChangeOp::SetRemote,
                                sd.serde.sdp_type,
                            );
                            if next_state.is_ok() {
                                self.pending_remote_description = None;
                            }
                            next_state
                        }
                        // have-local-offer->SetRemote(pranswer)->have-remote-pranswer
                        SDPType::Pranswer => {
                            let next_state = check_next_signaling_state(
                                cur,
                                SignalingState::HaveRemotePranswer,
                                StateChangeOp::SetRemote,
                                sd.serde.sdp_type,
                            );
                            if next_state.is_ok() {
                                self.pending_remote_description = Some(sd.clone());
                            }
                            next_state
                        }
                        _ => Err(Error::ErrPeerConnStateChangeInvalid.into()),
                    }
                } //_ => Err(Error::ErrPeerConnStateChangeUnhandled.into()),
            }
        };

        match next_state {
            Ok(next_state) => {
                self.signaling_state
                    .store(next_state as u8, Ordering::SeqCst);
                if self.signaling_state() == SignalingState::Stable {
                    self.is_negotiation_needed.store(false, Ordering::SeqCst);
                    self.do_negotiation_needed().await;
                }
                self.do_signaling_state_change(next_state).await;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// set_local_description sets the SessionDescription of the local peer
    pub async fn set_local_description(&mut self, mut desc: SessionDescription) -> Result<()> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed.into());
        }

        let _have_local_description = self.current_local_description.is_some();

        // JSEP 5.4
        if desc.serde.sdp.is_empty() {
            match desc.serde.sdp_type {
                SDPType::Answer | SDPType::Pranswer => {
                    desc.serde.sdp = self.last_answer.clone();
                }
                SDPType::Offer => {
                    desc.serde.sdp = self.last_offer.clone();
                }
                _ => return Err(Error::ErrPeerConnSDPTypeInvalidValueSetLocalDescription.into()),
            }
        }

        desc.parsed = Some(desc.unmarshal()?);
        self.set_description(&desc, StateChangeOp::SetLocal).await?;

        let current_transceivers = self.get_transceivers();

        let we_answer = desc.serde.sdp_type == SDPType::Answer;
        let remote_desc = self.remote_description();
        if we_answer && remote_desc.is_some() {
            self.start_rtp_senders(current_transceivers).await?;
            /*TODO:pc.ops.Enqueue(func() {
                pc.start_rtp(have_local_description, remote_desc, current_transceivers)
            })*/
        }

        if self.ice_gatherer.state() == ICEGathererState::New {
            self.ice_gatherer.gather().await
        } else {
            Ok(())
        }
    }

    /// local_description returns PendingLocalDescription if it is not null and
    /// otherwise it returns CurrentLocalDescription. This property is used to
    /// determine if set_local_description has already been called.
    /// https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-localdescription
    pub async fn local_description(&self) -> Option<SessionDescription> {
        if let Some(pending_local_description) = self.pending_local_description().await {
            return Some(pending_local_description);
        }
        self.current_local_description().await
    }

    /// set_remote_description sets the SessionDescription of the remote peer
    pub async fn set_remote_description(&mut self, mut desc: SessionDescription) -> Result<()> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed.into());
        }

        let is_renegotation = self.current_remote_description.is_some();

        desc.parsed = Some(desc.unmarshal()?);
        self.set_description(&desc, StateChangeOp::SetRemote)
            .await?;

        if let Some(parsed) = &desc.parsed {
            self.media_engine
                .update_from_remote_description(parsed)
                .await?;

            let mut local_transceivers = self.get_transceivers().to_vec();
            let detected_plan_b = description_is_plan_b(self.remote_description())?;
            let we_offer = desc.serde.sdp_type == SDPType::Answer;

            if !we_offer && !detected_plan_b {
                let desc = self.remote_description().cloned();
                if let Some(desc) = desc {
                    if let Some(parsed) = &desc.parsed {
                        for media in &parsed.media_descriptions {
                            if let Some(mid_value) = get_mid_value(media) {
                                if mid_value.is_empty() {
                                    return Err(
                                        Error::ErrPeerConnRemoteDescriptionWithoutMidValue.into()
                                    );
                                }

                                if media.media_name.media == MEDIA_SECTION_APPLICATION {
                                    continue;
                                }

                                let kind = RTPCodecType::from(media.media_name.media.as_str());
                                let direction = get_peer_direction(media);
                                if kind == RTPCodecType::Unspecified
                                    || direction == RTPTransceiverDirection::Unspecified
                                {
                                    continue;
                                }

                                let t = if let Some(t) =
                                    find_by_mid(mid_value, &mut local_transceivers).await
                                {
                                    t.stop().await?;
                                    Some(t)
                                } else {
                                    satisfy_type_and_direction(
                                        kind,
                                        direction,
                                        &mut local_transceivers,
                                    )
                                    .await
                                };

                                if let Some(t) = t {
                                    if direction == RTPTransceiverDirection::Recvonly {
                                        if t.direction() == RTPTransceiverDirection::Sendrecv {
                                            t.set_direction(RTPTransceiverDirection::Sendonly);
                                        }
                                    } else if direction == RTPTransceiverDirection::Sendrecv
                                        && t.direction() == RTPTransceiverDirection::Sendonly
                                    {
                                        t.set_direction(RTPTransceiverDirection::Sendrecv);
                                    }

                                    if t.mid().await.is_empty() {
                                        t.set_mid(mid_value.to_owned()).await?;
                                    }
                                } else {
                                    let receiver = Arc::new(RTPReceiver::new(
                                        kind,
                                        Arc::clone(&self.dtls_transport),
                                        Arc::clone(&self.media_engine),
                                        self.interceptor.clone(),
                                    ));

                                    let local_direction =
                                        if direction == RTPTransceiverDirection::Recvonly {
                                            RTPTransceiverDirection::Sendonly
                                        } else {
                                            RTPTransceiverDirection::Recvonly
                                        };

                                    let t = Arc::new(RTPTransceiver::new(
                                        Some(receiver),
                                        None,
                                        local_direction,
                                        kind,
                                        vec![],
                                        Arc::clone(&self.media_engine),
                                    ));

                                    self.add_rtp_transceiver(Arc::clone(&t)).await;

                                    if t.mid().await.is_empty() {
                                        t.set_mid(mid_value.to_owned()).await?;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let (remote_ufrag, remote_pwd, candidates) = extract_ice_details(parsed).await?;

            if is_renegotation
                && self
                    .ice_transport
                    .have_remote_credentials_change(&remote_ufrag, &remote_pwd)
                    .await
            {
                // An ICE Restart only happens implicitly for a set_remote_description of type offer
                if !we_offer {
                    self.ice_transport.restart().await?;
                }

                self.ice_transport
                    .set_remote_credentials(remote_ufrag, remote_pwd)
                    .await?;
            }

            for candidate in candidates {
                self.ice_transport
                    .add_remote_candidate(Some(candidate))
                    .await?;
            }

            let current_transceivers = self.get_transceivers();

            if is_renegotation {
                if we_offer {
                    self.start_rtp_senders(current_transceivers).await?;
                    /*TODO: self.ops.Enqueue(func() {
                        self.start_rtp(true, &desc, current_transceivers)
                    })*/
                }
                return Ok(());
            }

            let mut remote_is_lite = false;
            for a in &parsed.attributes {
                if a.key.trim() == ATTR_KEY_ICELITE {
                    remote_is_lite = true;
                    break;
                }
            }

            //TODO: let (_fingerprint, _fingerprint_hash) = extract_fingerprint(parsed)?;

            let _ice_role =
            // If one of the agents is lite and the other one is not, the lite agent must be the controlling agent.
            // If both or neither agents are lite the offering agent is controlling.
            // RFC 8445 S6.1.1
            if (we_offer && remote_is_lite == self.setting_engine.candidates.ice_lite)
                || (remote_is_lite && !self.setting_engine.candidates.ice_lite)
            {
                ICERole::Controlling
            }else{
                ICERole::Controlled
            };

            // Start the networking in a new routine since it will block until
            // the connection is actually established.
            if we_offer {
                self.start_rtp_senders(current_transceivers).await?;
            }

            /*TODO: self.ops.Enqueue(func() {
                self.start_transports(iceRole, dtlsRoleFromRemoteSDP(desc.parsed), remote_ufrag, remote_pwd, fingerprint, fingerprintHash)
                if weOffer {
                    self.start_rtp(false, &desc, current_transceivers)
                }
            })*/
        }

        Ok(())
    }

    async fn start_receiver(&self, incoming: &TrackDetails, receiver: Arc<RTPReceiver>) {
        let mut encodings = vec![];
        if incoming.ssrc != 0 {
            encodings.push(RTPCodingParameters {
                ssrc: incoming.ssrc,
                ..Default::default()
            });
        }
        for rid in &incoming.rids {
            encodings.push(RTPCodingParameters {
                rid: rid.to_owned(),
                ..Default::default()
            });
        }

        if let Err(err) = receiver.receive(&RTPReceiveParameters { encodings }).await {
            log::warn!("RTPReceiver Receive failed {}", err);
            return;
        }

        // set track id and label early so they can be set as new track information
        // is received from the SDP.
        for track_remote in &receiver.tracks().await {
            track_remote.set_id(incoming.id.clone()).await;
            track_remote.set_stream_id(incoming.stream_id.clone()).await;
        }

        // We can't block and wait for a single SSRC
        if incoming.ssrc == 0 {
            return;
        }

        let media_engine = Arc::clone(&self.media_engine);
        tokio::spawn(async move {
            if let Some(track) = receiver.track().await {
                if let Err(err) = track.determine_payload_type().await {
                    log::warn!(
                        "Could not determine PayloadType for SSRC {} with err {}",
                        track.ssrc(),
                        err
                    );
                    return;
                }

                let params = match media_engine
                    .get_rtp_parameters_by_payload_type(track.payload_type())
                    .await
                {
                    Ok(params) => params,
                    Err(err) => {
                        log::warn!(
                            "no codec could be found for payloadType {} with err {}",
                            track.payload_type(),
                            err,
                        );
                        return;
                    }
                };

                track.set_kind(receiver.kind());
                track.set_codec(params.codecs[0].clone()).await;
                track.set_params(params).await;

                //TODO:self.do_track(receiver.track().await, Some(Arc::clone(&receiver))).await;
            }
        });
    }

    /// start_rtp_receivers opens knows inbound SRTP streams from the remote_description
    pub(crate) async fn start_rtp_receivers(
        &mut self,
        incoming_tracks: &mut Vec<TrackDetails>,
        current_transceivers: &[Arc<RTPTransceiver>],
    ) -> Result<()> {
        let local_transceivers = current_transceivers.to_vec();

        let remote_is_plan_b = match self.configuration.sdp_semantics {
            SDPSemantics::PlanB => true,
            SDPSemantics::UnifiedPlanWithFallback => {
                description_is_plan_b(self.remote_description())?
            }
            _ => false,
        };

        // Ensure we haven't already started a transceiver for this ssrc
        let ssrcs: Vec<SSRC> = incoming_tracks.iter().map(|x| x.ssrc).collect();
        for ssrc in ssrcs {
            for t in &local_transceivers {
                if let Some(receiver) = t.receiver().await {
                    if let Some(track) = receiver.track().await {
                        if track.ssrc() != ssrc {
                            continue;
                        }
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }

                filter_track_with_ssrc(incoming_tracks, ssrc);
            }
        }

        let mut unhandled_tracks = vec![]; // incoming_tracks[:0]
        for incoming_track in incoming_tracks.iter() {
            let mut track_handled = false;
            for t in &local_transceivers {
                if t.mid().await != incoming_track.mid {
                    continue;
                }

                if (incoming_track.kind != t.kind())
                    || (t.direction() != RTPTransceiverDirection::Recvonly
                        && t.direction() != RTPTransceiverDirection::Sendrecv)
                {
                    continue;
                }

                if let Some(receiver) = t.receiver().await {
                    if receiver.have_received().await {
                        continue;
                    }
                    self.start_receiver(incoming_track, receiver).await;
                    track_handled = true;
                }
            }

            if !track_handled {
                unhandled_tracks.push(incoming_track);
            }
        }

        if remote_is_plan_b {
            for incoming in unhandled_tracks {
                let t = match self
                    .add_transceiver_from_kind(
                        incoming.kind,
                        &[RTPTransceiverInit {
                            direction: RTPTransceiverDirection::Sendrecv,
                            send_encodings: vec![],
                        }],
                    )
                    .await
                {
                    Ok(t) => t,
                    Err(err) => {
                        log::warn!(
                            "Could not add transceiver for remote SSRC {}: {}",
                            incoming.ssrc,
                            err
                        );
                        continue;
                    }
                };
                if let Some(receiver) = t.receiver().await {
                    self.start_receiver(incoming, receiver).await;
                }
            }
        }

        Ok(())
    }

    /// start_rtp_senders starts all outbound RTP streams
    pub(crate) async fn start_rtp_senders(
        &self,
        current_transceivers: &[Arc<RTPTransceiver>],
    ) -> Result<()> {
        for transceiver in current_transceivers {
            if let Some(sender) = transceiver.sender().await {
                if sender.is_negotiated() && !sender.has_sent().await {
                    sender.send(&sender.get_parameters().await).await?;
                }
            }
        }

        Ok(())
    }

    /// Start SCTP subsystem
    pub(crate) async fn start_sctp(&self) {
        // Start sctp
        if let Err(err) = self
            .sctp_transport
            .start(SCTPTransportCapabilities {
                max_message_size: 0,
            })
            .await
        {
            log::warn!("Failed to start SCTP: {}", err);
            if let Err(err) = self.sctp_transport.stop().await {
                log::warn!("Failed to stop SCTPTransport: {}", err);
            }

            return;
        }

        // DataChannels that need to be opened now that SCTP is available
        // make a copy we may have incoming DataChannels mutating this while we open
        let data_channels = {
            let data_channels = self.sctp_transport.data_channels.lock().await;
            data_channels.clone()
        };

        let mut opened_dc_count = 0;
        for d in data_channels {
            if d.ready_state() == DataChannelState::Connecting {
                if let Err(err) = d.open(Arc::clone(&self.sctp_transport)).await {
                    log::warn!("failed to open data channel: {}", err);
                    continue;
                }
                opened_dc_count += 1;
            }
        }

        self.sctp_transport
            .data_channels_opened
            .fetch_add(opened_dc_count, Ordering::SeqCst);
    }

    async fn handle_undeclared_ssrc<R: Read>(
        &mut self,
        rtp_stream: &mut R,
        ssrc: SSRC,
    ) -> Result<()> {
        if let Some(remote_description) = self.remote_description() {
            if let Some(parsed) = &remote_description.parsed {
                // If the remote SDP was only one media section the ssrc doesn't have to be explicitly declared
                if parsed.media_descriptions.len() == 1 {
                    if let Some(only_media_section) = parsed.media_descriptions.first() {
                        for a in &only_media_section.attributes {
                            if a.key == SSRC_STR {
                                return Err(
                                    Error::ErrPeerConnSingleMediaSectionHasExplicitSSRC.into()
                                );
                            }
                        }

                        let mut incoming = TrackDetails {
                            ssrc,
                            kind: RTPCodecType::Video,
                            ..Default::default()
                        };
                        if only_media_section.media_name.media == RTPCodecType::Audio.to_string() {
                            incoming.kind = RTPCodecType::Audio;
                        }

                        let t = self
                            .add_transceiver_from_kind(
                                incoming.kind,
                                &[RTPTransceiverInit {
                                    direction: RTPTransceiverDirection::Sendrecv,
                                    send_encodings: vec![],
                                }],
                            )
                            .await?;

                        if let Some(receiver) = t.receiver().await {
                            self.start_receiver(&incoming, receiver).await;
                        }
                        return Ok(());
                    }
                }

                let (mid_extension_id, audio_supported, video_supported) = self
                    .media_engine
                    .get_header_extension_id(RTPHeaderExtensionCapability {
                        uri: sdp::extmap::SDES_MID_URI.to_owned(),
                    })
                    .await;
                if !audio_supported && !video_supported {
                    return Err(Error::ErrPeerConnSimulcastMidRTPExtensionRequired.into());
                }

                let (sid_extension_id, audio_supported, video_supported) = self
                    .media_engine
                    .get_header_extension_id(RTPHeaderExtensionCapability {
                        uri: sdp::extmap::SDES_RTP_STREAM_ID_URI.to_owned(),
                    })
                    .await;
                if !audio_supported && !video_supported {
                    return Err(Error::ErrPeerConnSimulcastStreamIDRTPExtensionRequired.into());
                }

                let mut b = vec![0u8; RECEIVE_MTU];
                let (mut mid, mut rid) = (String::new(), String::new());
                for _ in 0..=SIMULCAST_PROBE_COUNT {
                    let n = rtp_stream.read(&mut b)?;

                    let (maybe_mid, maybe_rid, payload_type) = handle_unknown_rtp_packet(
                        &b[..n],
                        mid_extension_id as u8,
                        sid_extension_id as u8,
                    )?;

                    if !maybe_mid.is_empty() {
                        mid = maybe_mid;
                    }
                    if !maybe_rid.is_empty() {
                        rid = maybe_rid;
                    }

                    if mid.is_empty() || rid.is_empty() {
                        continue;
                    }

                    let params = self
                        .media_engine
                        .get_rtp_parameters_by_payload_type(payload_type)
                        .await?;

                    for t in self.get_transceivers() {
                        if t.mid().await != mid || t.receiver().await.is_none() {
                            continue;
                        }

                        if let Some(receiver) = t.receiver().await {
                            let track = receiver
                                .receive_for_rid(rid.as_str(), &params, ssrc)
                                .await?;
                            self.do_track(Some(track), Some(receiver.clone())).await;
                        }
                        return Ok(());
                    }
                }
                return Err(Error::ErrPeerConnSimulcastIncomingSSRCFailed.into());
            }
        }

        Err(Error::ErrPeerConnRemoteDescriptionNil.into())
    }

    /// undeclared_media_processor handles RTP/RTCP packets that don't match any a:ssrc lines
    fn undeclared_media_processor(&self) {
        let dtls_transport = Arc::clone(&self.dtls_transport);
        let is_closed = Arc::clone(&self.is_closed);
        tokio::spawn(async move {
            let simulcast_routine_count = Arc::new(AtomicU64::new(0));
            loop {
                let srtp_session = match dtls_transport.get_srtp_session().await {
                    Some(s) => s,
                    None => {
                        log::warn!("undeclared_media_processor failed to open SrtpSession");
                        return;
                    }
                };

                let stream = match srtp_session.accept().await {
                    Ok(stream) => Arc::new(stream),
                    Err(err) => {
                        log::warn!("Failed to accept RTP {}", err);
                        return;
                    }
                };

                if is_closed.load(Ordering::SeqCst) {
                    if let Err(err) = stream.close().await {
                        log::warn!("Failed to close RTP stream {}", err);
                    }
                    continue;
                }

                if simulcast_routine_count.fetch_add(1, Ordering::SeqCst) + 1
                    >= SIMULCAST_MAX_PROBE_ROUTINES
                {
                    simulcast_routine_count.fetch_sub(1, Ordering::SeqCst);
                    log::warn!("{:?}", Error::ErrSimulcastProbeOverflow);
                    continue;
                }

                let dtls_transport2 = Arc::clone(&dtls_transport);
                let simulcast_routine_count2 = Arc::clone(&simulcast_routine_count);
                tokio::spawn(async move {
                    dtls_transport2
                        .store_simulcast_stream(Arc::clone(&stream))
                        .await;

                    let _ssrc = stream.get_ssrc();
                    /*TODO:if let Err(err) = self.handle_undeclared_ssrc(stream, ssrc) {
                        log::error!(
                            "Incoming unhandled RTP ssrc({}), on_track will not be fired. {}",
                            ssrc,
                            err
                        );
                    }*/

                    simulcast_routine_count2.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });

        let dtls_transport = Arc::clone(&self.dtls_transport);
        tokio::spawn(async move {
            loop {
                let srtcp_session = match dtls_transport.get_srtcp_session().await {
                    Some(s) => s,
                    None => {
                        log::warn!("undeclared_media_processor failed to open SrtcpSession");
                        return;
                    }
                };

                let stream = match srtcp_session.accept().await {
                    Ok(stream) => stream,
                    Err(err) => {
                        log::warn!("Failed to accept RTCP {}", err);
                        return;
                    }
                };
                log::warn!(
                    "Incoming unhandled RTCP ssrc({}), on_track will not be fired",
                    stream.get_ssrc()
                );
            }
        });
    }

    /// remote_description returns pending_remote_description if it is not null and
    /// otherwise it returns current_remote_description. This property is used to
    /// determine if setRemoteDescription has already been called.
    /// https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-remotedescription
    pub fn remote_description(&self) -> Option<&SessionDescription> {
        if self.pending_remote_description.is_some() {
            self.pending_remote_description.as_ref()
        } else {
            self.current_remote_description.as_ref()
        }
    }

    /// add_ice_candidate accepts an ICE candidate string and adds it
    /// to the existing set of candidates.
    pub async fn add_ice_candidate(&self, candidate: ICECandidateInit) -> Result<()> {
        if self.remote_description().is_none() {
            return Err(Error::ErrNoRemoteDescription.into());
        }

        let candidate_value = match candidate.candidate.strip_prefix("candidate:") {
            Some(s) => s,
            None => candidate.candidate.as_str(),
        };

        let ice_candidate = if !candidate_value.is_empty() {
            let candidate: Arc<dyn Candidate + Send + Sync> =
                Arc::new(unmarshal_candidate(candidate_value).await?);

            Some(ICECandidate::from(&candidate))
        } else {
            None
        };

        self.ice_transport.add_remote_candidate(ice_candidate).await
    }

    /// ice_connection_state returns the ICE connection state of the
    /// PeerConnection instance.
    pub fn ice_connection_state(&self) -> ICEConnectionState {
        self.ice_connection_state.load(Ordering::SeqCst).into()
    }

    /// get_senders returns the RTPSender that are currently attached to this PeerConnection
    pub async fn get_senders(&self) -> Vec<Arc<RTPSender>> {
        let mut senders = vec![];
        for transceiver in &self.rtp_transceivers {
            if let Some(sender) = transceiver.sender().await {
                senders.push(sender);
            }
        }
        senders
    }

    /// get_receivers returns the RTPReceivers that are currently attached to this PeerConnection
    pub async fn get_receivers(&self) -> Vec<Arc<RTPReceiver>> {
        let mut receivers = vec![];
        for transceiver in &self.rtp_transceivers {
            if let Some(receiver) = transceiver.receiver().await {
                receivers.push(receiver);
            }
        }
        receivers
    }

    /// get_transceivers returns the RtpTransceiver that are currently attached to this PeerConnection
    pub fn get_transceivers(&self) -> &[Arc<RTPTransceiver>] {
        &self.rtp_transceivers
    }

    /// add_track adds a Track to the PeerConnection
    pub async fn add_track(
        &mut self,
        track: Arc<dyn TrackLocal + Send + Sync>,
    ) -> Result<Arc<RTPSender>> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed.into());
        }

        for t in &self.rtp_transceivers {
            if !t.stopped && t.kind == track.kind() && t.sender().await.is_none() {
                let sender = Arc::new(RTPSender::new(
                    Arc::clone(&track),
                    Arc::clone(&self.dtls_transport),
                    Arc::clone(&self.media_engine),
                    self.interceptor.clone(),
                ));

                if let Err(err) = t
                    .set_sender_track(Some(Arc::clone(&sender)), Some(Arc::clone(&track)))
                    .await
                {
                    let _ = sender.stop().await;
                    let _ = t.set_sender(None).await;
                    return Err(err);
                }

                self.do_negotiation_needed().await;
                return Ok(sender);
            }
        }

        let transceiver =
            self.new_transceiver_from_track(RTPTransceiverDirection::Sendrecv, track)?;
        self.add_rtp_transceiver(Arc::clone(&transceiver)).await;

        match transceiver.sender().await {
            Some(sender) => Ok(sender),
            None => Err(Error::ErrRTPSenderNil.into()),
        }
    }

    /// remove_track removes a Track from the PeerConnection
    pub async fn remove_track(&self, sender: &Arc<RTPSender>) -> Result<()> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed.into());
        }

        let mut transceiver = None;
        for t in &self.rtp_transceivers {
            if let Some(s) = t.sender().await {
                if s.id == sender.id {
                    transceiver = Some(t);
                    break;
                }
            }
        }

        if let Some(t) = transceiver {
            if sender.stop().await.is_ok() && t.set_sending_track(None).await.is_ok() {
                self.do_negotiation_needed().await;
            }
            Ok(())
        } else {
            Err(Error::ErrSenderNotCreatedByConnection.into())
        }
    }

    fn new_transceiver_from_track(
        &self,
        direction: RTPTransceiverDirection,
        track: Arc<dyn TrackLocal + Send + Sync>,
    ) -> Result<Arc<RTPTransceiver>> {
        let (r, s) = match direction {
            RTPTransceiverDirection::Sendrecv => {
                let r = Some(Arc::new(RTPReceiver::new(
                    track.kind(),
                    Arc::clone(&self.dtls_transport),
                    Arc::clone(&self.media_engine),
                    self.interceptor.clone(),
                )));
                let s = Some(Arc::new(RTPSender::new(
                    Arc::clone(&track),
                    Arc::clone(&self.dtls_transport),
                    Arc::clone(&self.media_engine),
                    self.interceptor.clone(),
                )));
                (r, s)
            }
            RTPTransceiverDirection::Sendonly => {
                let s = Some(Arc::new(RTPSender::new(
                    Arc::clone(&track),
                    Arc::clone(&self.dtls_transport),
                    Arc::clone(&self.media_engine),
                    self.interceptor.clone(),
                )));
                (None, s)
            }
            _ => return Err(Error::ErrPeerConnAddTransceiverFromTrackSupport.into()),
        };

        Ok(Arc::new(RTPTransceiver::new(
            r,
            s,
            direction,
            track.kind(),
            vec![],
            Arc::clone(&self.media_engine),
        )))
    }

    /// add_transceiver_from_kind Create a new RtpTransceiver and adds it to the set of transceivers.
    pub async fn add_transceiver_from_kind(
        &mut self,
        kind: RTPCodecType,
        init: &[RTPTransceiverInit],
    ) -> Result<Arc<RTPTransceiver>> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed.into());
        }

        let direction = match init.len() {
            0 => RTPTransceiverDirection::Sendrecv,
            1 => init[0].direction,
            _ => return Err(Error::ErrPeerConnAddTransceiverFromKindOnlyAcceptsOne.into()),
        };

        let t = match direction {
            RTPTransceiverDirection::Sendonly | RTPTransceiverDirection::Sendrecv => {
                let codecs = self.media_engine.get_codecs_by_kind(kind).await;
                if codecs.is_empty() {
                    return Err(Error::ErrNoCodecsAvailable.into());
                }
                let track = Arc::new(TrackLocalStaticSample::new(
                    codecs[0].capability.clone(),
                    math_rand_alpha(16),
                    math_rand_alpha(16),
                ));

                self.new_transceiver_from_track(direction, track)?
            }
            RTPTransceiverDirection::Recvonly => {
                let receiver = Arc::new(RTPReceiver::new(
                    kind,
                    Arc::clone(&self.dtls_transport),
                    Arc::clone(&self.media_engine),
                    self.interceptor.clone(),
                ));

                Arc::new(RTPTransceiver::new(
                    Some(receiver),
                    None,
                    RTPTransceiverDirection::Recvonly,
                    kind,
                    vec![],
                    Arc::clone(&self.media_engine),
                ))
            }
            _ => return Err(Error::ErrPeerConnAddTransceiverFromKindSupport.into()),
        };

        self.add_rtp_transceiver(Arc::clone(&t)).await;

        Ok(t)
    }

    /// add_transceiver_from_track Create a new RtpTransceiver(SendRecv or SendOnly) and add it to the set of transceivers.
    pub async fn add_transceiver_from_track(
        &mut self,
        track: &Arc<dyn TrackLocal + Send + Sync>, //Why compiler complains if "track: Arc<dyn TrackLocal + Send + Sync>"?
        init: &[RTPTransceiverInit],
    ) -> Result<Arc<RTPTransceiver>> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed.into());
        }

        let direction = match init.len() {
            0 => RTPTransceiverDirection::Sendrecv,
            1 => init[0].direction,
            _ => return Err(Error::ErrPeerConnAddTransceiverFromTrackOnlyAcceptsOne.into()),
        };

        let t = self.new_transceiver_from_track(direction, Arc::clone(track))?;

        self.add_rtp_transceiver(Arc::clone(&t)).await;

        Ok(t)
    }

    /// create_data_channel creates a new DataChannel object with the given label
    /// and optional DataChannelInit used to configure properties of the
    /// underlying channel such as data reliability.
    pub async fn create_data_channel(
        &self,
        label: &str,
        options: Option<DataChannelConfig>,
    ) -> Result<Arc<DataChannel>> {
        // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #2)
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed.into());
        }

        let mut params = DataChannelParameters {
            label: label.to_owned(),
            ordered: true,
            ..Default::default()
        };

        // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #19)
        if let Some(options) = options {
            if let Some(id) = options.id {
                params.id = id;
            }

            // Ordered indicates if data is allowed to be delivered out of order. The
            // default value of true, guarantees that data will be delivered in order.
            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #9)
            if let Some(ordered) = options.ordered {
                params.ordered = ordered;
            }

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #7)
            if let Some(max_packet_life_time) = options.max_packet_life_time {
                params.max_packet_life_time = max_packet_life_time;
            }

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #8)
            if let Some(max_retransmits) = options.max_retransmits {
                params.max_retransmits = max_retransmits;
            }

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #10)
            if let Some(protocol) = options.protocol {
                params.protocol = protocol;
            }

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #11)
            if params.protocol.len() > 65535 {
                return Err(Error::ErrProtocolTooLarge.into());
            }

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #12)
            if let Some(negotiated) = options.negotiated {
                params.negotiated = negotiated;
            }
        }

        let d = Arc::new(DataChannel::new(params, Arc::clone(&self.setting_engine)));

        // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #16)
        if d.max_packet_lifetime != 0 && d.max_retransmits != 0 {
            return Err(Error::ErrRetransmitsOrPacketLifeTime.into());
        }

        {
            let mut data_channels = self.sctp_transport.data_channels.lock().await;
            data_channels.push(Arc::clone(&d));
        }
        self.sctp_transport
            .data_channels_requested
            .fetch_add(1, Ordering::SeqCst);

        // If SCTP already connected open all the channels
        if self.sctp_transport.state() == SCTPTransportState::Connected {
            d.open(Arc::clone(&self.sctp_transport)).await?;
        }

        self.do_negotiation_needed().await;

        Ok(d)
    }

    /// set_identity_provider is used to configure an identity provider to generate identity assertions
    pub fn set_identity_provider(&self, _provider: &str) -> Result<()> {
        Err(Error::ErrPeerConnSetIdentityProviderNotImplemented.into())
    }

    /// write_rtcp sends a user provided RTCP packet to the connected peer. If no peer is connected the
    /// packet is discarded. It also runs any configured interceptors.
    pub async fn write_rtcp(&self, pkts: &dyn rtcp::packet::Packet) -> Result<()> {
        if let Some(rtc_writer) = &self.interceptor_rtcp_writer {
            let a = Attributes::new();
            rtc_writer.write(pkts, &a).await?;
        }
        Ok(())
    }

    async fn dtls_write_rtcp(
        &self,
        pkts: &dyn rtcp::packet::Packet,
        _a: &Attributes,
    ) -> Result<usize> {
        self.dtls_transport.write_rtcp(pkts).await
    }

    /// close ends the PeerConnection
    pub async fn close(&self) -> Result<()> {
        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #1)
        if self.is_closed.load(Ordering::SeqCst) {
            return Ok(());
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #2)
        self.is_closed.store(true, Ordering::SeqCst);

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #3)
        self.signaling_state
            .store(SignalingState::Closed as u8, Ordering::SeqCst);

        // Try closing everything and collect the errors
        // Shutdown strategy:
        // 1. All Conn close by closing their underlying Conn.
        // 2. A Mux stops this chain. It won't close the underlying
        //    Conn if one of the endpoints is closed down. To
        //    continue the chain the Mux has to be closed.
        let mut close_errs = vec![];

        if let Some(interceptor) = &self.interceptor {
            if let Err(err) = interceptor.close().await {
                close_errs.push(err);
            }
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #4)
        for t in &self.rtp_transceivers {
            if !t.stopped {
                if let Err(err) = t.stop().await {
                    close_errs.push(err);
                }
            }
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #5)
        {
            let data_channels = self.sctp_transport.data_channels.lock().await;
            for d in &*data_channels {
                d.set_ready_state(DataChannelState::Closed);
            }
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #6)
        if let Err(err) = self.sctp_transport.stop().await {
            close_errs.push(err);
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #7)
        if let Err(err) = self.dtls_transport.stop().await {
            close_errs.push(err);
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #8, #9, #10)
        if let Err(err) = self.ice_transport.stop().await {
            close_errs.push(err);
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #11)
        PeerConnection::update_connection_state(
            &self.on_peer_connection_state_change_handler,
            &self.is_closed,
            &self.peer_connection_state,
            self.ice_connection_state(),
            self.dtls_transport.state(),
        )
        .await;

        if let Err(err) = self.ops.close().await {
            close_errs.push(err);
        }

        flatten_errs(close_errs)
    }

    /// add_rtp_transceiver appends t into rtp_transceivers
    /// and fires onNegotiationNeeded;
    /// caller of this method should hold `self.mu` lock
    async fn add_rtp_transceiver(&mut self, t: Arc<RTPTransceiver>) {
        self.rtp_transceivers.push(t);
        self.do_negotiation_needed().await;
    }

    /// CurrentLocalDescription represents the local description that was
    /// successfully negotiated the last time the PeerConnection transitioned
    /// into the stable state plus any local candidates that have been generated
    /// by the ICEAgent since the offer or answer was created.
    pub async fn current_local_description(&self) -> Option<SessionDescription> {
        let local_description = self.current_local_description.as_ref();
        let ice_gather = Some(&self.ice_gatherer);
        let ice_gathering_state = self.ice_gathering_state();

        populate_local_candidates(local_description, ice_gather, ice_gathering_state).await
    }

    /// PendingLocalDescription represents a local description that is in the
    /// process of being negotiated plus any local candidates that have been
    /// generated by the ICEAgent since the offer or answer was created. If the
    /// PeerConnection is in the stable state, the value is null.
    pub async fn pending_local_description(&self) -> Option<SessionDescription> {
        let local_description = self.pending_local_description.as_ref();
        let ice_gather = Some(&self.ice_gatherer);
        let ice_gathering_state = self.ice_gathering_state();

        populate_local_candidates(local_description, ice_gather, ice_gathering_state).await
    }

    /// current_remote_description represents the last remote description that was
    /// successfully negotiated the last time the PeerConnection transitioned
    /// into the stable state plus any remote candidates that have been supplied
    /// via add_icecandidate() since the offer or answer was created.
    pub fn current_remote_description(&self) -> Option<&SessionDescription> {
        self.current_remote_description.as_ref()
    }

    /// pending_remote_description represents a remote description that is in the
    /// process of being negotiated, complete with any remote candidates that
    /// have been supplied via add_icecandidate() since the offer or answer was
    /// created. If the PeerConnection is in the stable state, the value is
    /// null.
    pub fn pending_remote_description(&self) -> Option<&SessionDescription> {
        self.pending_remote_description.as_ref()
    }

    /// signaling_state attribute returns the signaling state of the
    /// PeerConnection instance.
    pub fn signaling_state(&self) -> SignalingState {
        self.signaling_state.load(Ordering::SeqCst).into()
    }

    /// icegathering_state attribute returns the ICE gathering state of the
    /// PeerConnection instance.
    pub fn ice_gathering_state(&self) -> ICEGatheringState {
        match self.ice_gatherer.state() {
            ICEGathererState::New => ICEGatheringState::New,
            ICEGathererState::Gathering => ICEGatheringState::Gathering,
            _ => ICEGatheringState::Complete,
        }
    }

    /// connection_state attribute returns the connection state of the
    /// PeerConnection instance.
    pub fn connection_state(&self) -> PeerConnectionState {
        self.peer_connection_state.load(Ordering::SeqCst).into()
    }

    // GetStats return data providing statistics about the overall connection
    /*TODO: func (pc *PeerConnection) GetStats() StatsReport {
        var (
            dataChannelsAccepted  uint32
            dataChannelsClosed    uint32
            dataChannelsOpened    uint32
            dataChannelsRequested uint32
        )
        statsCollector := newStatsReportCollector()
        statsCollector.Collecting()

        self.mu.Lock()
        if self.iceGatherer != nil {
            self.iceGatherer.collectStats(statsCollector)
        }
        if self.iceTransport != nil {
            self.iceTransport.collectStats(statsCollector)
        }

        self.sctpTransport.lock.Lock()
        dataChannels := append([]*DataChannel{}, self.sctpTransport.dataChannels...)
        dataChannelsAccepted = self.sctpTransport.dataChannelsAccepted
        dataChannelsOpened = self.sctpTransport.dataChannelsOpened
        dataChannelsRequested = self.sctpTransport.dataChannelsRequested
        self.sctpTransport.lock.Unlock()

        for _, d := range dataChannels {
            state := d.ReadyState()
            if state != DataChannelStateConnecting && state != DataChannelStateOpen {
                dataChannelsClosed++
            }

            d.collectStats(statsCollector)
        }
        self.sctpTransport.collectStats(statsCollector)

        stats := PeerConnectionStats{
            Timestamp:             statsTimestampNow(),
            Type:                  StatsTypePeerConnection,
            ID:                    self.stats_id,
            DataChannelsAccepted:  dataChannelsAccepted,
            DataChannelsClosed:    dataChannelsClosed,
            DataChannelsOpened:    dataChannelsOpened,
            DataChannelsRequested: dataChannelsRequested,
        }

        statsCollector.Collect(stats.ID, stats)

        certificates := self.configuration.Certificates
        for _, certificate := range certificates {
            if err := certificate.collectStats(statsCollector); err != nil {
                continue
            }
        }
        self.mu.Unlock()

        self.api.mediaEngine.collectStats(statsCollector)

        return statsCollector.Ready()
    }
    */

    /// Start all transports. PeerConnection now has enough state
    async fn start_transports(
        params: StartTransportsParams,
        ice_role: ICERole,
        dtls_role: DTLSRole,
        remote_ufrag: String,
        remote_pwd: String,
        fingerprint: String,
        fingerprint_hash: String,
    ) {
        // Start the ice transport
        if let Err(err) = params
            .ice_transport
            .start(
                ICEParameters {
                    username_fragment: remote_ufrag,
                    password: remote_pwd,
                    ice_lite: false,
                },
                Some(ice_role),
            )
            .await
        {
            log::warn!("Failed to start manager: {}", err);
            return;
        }

        // Start the dtls_transport transport
        let result = params
            .dtls_transport
            .start(DTLSParameters {
                role: dtls_role,
                fingerprints: vec![DTLSFingerprint {
                    algorithm: fingerprint_hash,
                    value: fingerprint,
                }],
            })
            .await;
        PeerConnection::update_connection_state(
            &params.on_peer_connection_state_change_handler,
            &params.is_closed,
            &params.peer_connection_state,
            params.ice_connection_state.load(Ordering::SeqCst).into(),
            params.dtls_transport.state(),
        )
        .await;
        if let Err(err) = result {
            log::warn!("Failed to start manager: {}", err);
        }
    }

    async fn start_rtp(
        &mut self,
        is_renegotiation: bool,
        remote_desc: &SessionDescription,
        current_transceivers: &[Arc<RTPTransceiver>],
    ) -> Result<()> {
        let mut track_details = if let Some(parsed) = &remote_desc.parsed {
            track_details_from_sdp(parsed)
        } else {
            vec![]
        };

        if is_renegotiation {
            for t in current_transceivers {
                if let Some(receiver) = t.receiver().await {
                    if let Some(track) = receiver.track().await {
                        let ssrc = track.ssrc();
                        if let Some(details) = track_details_for_ssrc(&track_details, ssrc) {
                            track.set_id(details.id.clone()).await;
                            track.set_stream_id(details.stream_id.clone()).await;
                            continue;
                        }
                    }

                    if let Err(err) = receiver.stop().await {
                        log::warn!("Failed to stop RtpReceiver: {}", err);
                        continue;
                    }

                    let receiver = Arc::new(RTPReceiver::new(
                        receiver.kind(),
                        Arc::clone(&self.dtls_transport),
                        Arc::clone(&self.media_engine),
                        self.interceptor.clone(),
                    ));
                    t.set_receiver(Some(receiver)).await;
                }
            }
        }

        self.start_rtp_receivers(&mut track_details, current_transceivers)
            .await?;

        if let Some(parsed) = &remote_desc.parsed {
            if have_application_media_section(parsed) {
                self.start_sctp().await;
            }
        }

        if !is_renegotiation {
            self.undeclared_media_processor()
        }

        Ok(())
    }

    /// generate_unmatched_sdp generates an SDP that doesn't take remote state into account
    /// This is used for the initial call for CreateOffer
    async fn generate_unmatched_sdp(
        &self,
        use_identity: bool,
    ) -> Result<sdp::session_description::SessionDescription> {
        let transceivers = &self.rtp_transceivers;
        let d = sdp::session_description::SessionDescription::new_jsep_session_description(
            use_identity,
        );

        let ice_params = self.ice_gatherer.get_local_parameters().await?;

        let candidates = self.ice_gatherer.get_local_candidates().await?;

        let is_plan_b = self.configuration.sdp_semantics == SDPSemantics::PlanB;
        let mut media_sections = vec![];

        // Needed for self.sctpTransport.dataChannelsRequested
        if is_plan_b {
            let mut video = vec![];
            let mut audio = vec![];

            for t in transceivers {
                if t.kind == RTPCodecType::Video {
                    video.push(Arc::clone(t));
                } else if t.kind == RTPCodecType::Audio {
                    audio.push(Arc::clone(t));
                }
                if let Some(sender) = t.sender().await {
                    sender.set_negotiated();
                }
            }

            if !video.is_empty() {
                media_sections.push(MediaSection {
                    id: "video".to_owned(),
                    transceivers: video,
                    ..Default::default()
                })
            }
            if !audio.is_empty() {
                media_sections.push(MediaSection {
                    id: "audio".to_owned(),
                    transceivers: audio,
                    ..Default::default()
                });
            }

            if self
                .sctp_transport
                .data_channels_requested
                .load(Ordering::SeqCst)
                != 0
            {
                media_sections.push(MediaSection {
                    id: "data".to_owned(),
                    data: true,
                    ..Default::default()
                });
            }
        } else {
            for t in transceivers {
                if let Some(sender) = t.sender().await {
                    sender.set_negotiated();
                }
                media_sections.push(MediaSection {
                    id: t.mid().await,
                    transceivers: vec![Arc::clone(t)],
                    ..Default::default()
                });
            }

            if self
                .sctp_transport
                .data_channels_requested
                .load(Ordering::SeqCst)
                != 0
            {
                media_sections.push(MediaSection {
                    id: format!("{}", media_sections.len()),
                    data: true,
                    ..Default::default()
                });
            }
        }

        let dtls_fingerprints = vec![];
        /*TODO: dtls_fingerprints, err := self.configuration.Certificates[0].GetFingerprints()
        if err != nil {
            return nil, err
        }*/

        let params = PopulateSdpParams {
            is_plan_b,
            media_description_fingerprint: self.setting_engine.sdp_media_level_fingerprints,
            is_icelite: self.setting_engine.candidates.ice_lite,
            connection_role: DEFAULT_DTLS_ROLE_OFFER.to_connection_role(),
            ice_gathering_state: self.ice_gathering_state(),
        };
        populate_sdp(
            d,
            &dtls_fingerprints,
            &self.media_engine,
            &candidates,
            &ice_params,
            &media_sections,
            params,
        )
        .await
    }

    /// generate_matched_sdp generates a SDP and takes the remote state into account
    /// this is used everytime we have a remote_description
    async fn generate_matched_sdp(
        &self,
        use_identity: bool,
        include_unmatched: bool,
        connection_role: ConnectionRole,
    ) -> Result<sdp::session_description::SessionDescription> {
        let transceivers = &self.rtp_transceivers;

        let d = sdp::session_description::SessionDescription::new_jsep_session_description(
            use_identity,
        );

        let ice_params = self.ice_gatherer.get_local_parameters().await?;
        let candidates = self.ice_gatherer.get_local_candidates().await?;

        let remote_description = if self.pending_remote_description.is_some() {
            self.pending_remote_description.as_ref()
        } else {
            self.current_remote_description.as_ref()
        };

        let mut local_transceivers = transceivers.clone();
        let detected_plan_b = description_is_plan_b(remote_description)?;
        let mut media_sections = vec![];
        let mut already_have_application_media_section = false;
        if let Some(remote_description) = remote_description {
            if let Some(parsed) = &remote_description.parsed {
                for media in &parsed.media_descriptions {
                    if let Some(mid_value) = get_mid_value(media) {
                        if mid_value.is_empty() {
                            return Err(Error::ErrPeerConnRemoteDescriptionWithoutMidValue.into());
                        }

                        if media.media_name.media == MEDIA_SECTION_APPLICATION {
                            media_sections.push(MediaSection {
                                id: mid_value.to_owned(),
                                data: true,
                                ..Default::default()
                            });
                            already_have_application_media_section = true;
                            continue;
                        }

                        let kind = RTPCodecType::from(media.media_name.media.as_str());
                        let direction = get_peer_direction(media);
                        if kind == RTPCodecType::Unspecified
                            || direction == RTPTransceiverDirection::Unspecified
                        {
                            continue;
                        }

                        let sdp_semantics = self.configuration.sdp_semantics;

                        if sdp_semantics == SDPSemantics::PlanB
                            || (sdp_semantics == SDPSemantics::UnifiedPlanWithFallback
                                && detected_plan_b)
                        {
                            if !detected_plan_b {
                                return Err(Error::ErrIncorrectSDPSemantics.into());
                            }
                            // If we're responding to a plan-b offer, then we should try to fill up this
                            // media entry with all matching local transceivers
                            let mut media_transceivers = vec![];
                            loop {
                                // keep going until we can't get any more
                                if let Some(t) = satisfy_type_and_direction(
                                    kind,
                                    direction,
                                    &mut local_transceivers,
                                )
                                .await
                                {
                                    if let Some(sender) = t.sender().await {
                                        sender.set_negotiated();
                                    }
                                    media_transceivers.push(t);
                                } else {
                                    if media_transceivers.is_empty() {
                                        let t = RTPTransceiver::new(
                                            None,
                                            None,
                                            RTPTransceiverDirection::Inactive,
                                            kind,
                                            vec![],
                                            Arc::clone(&self.media_engine),
                                        );
                                        media_transceivers.push(Arc::new(t));
                                    }
                                    break;
                                }
                            }
                            media_sections.push(MediaSection {
                                id: mid_value.to_owned(),
                                transceivers: media_transceivers,
                                ..Default::default()
                            });
                        } else if sdp_semantics == SDPSemantics::UnifiedPlan
                            || sdp_semantics == SDPSemantics::UnifiedPlanWithFallback
                        {
                            if detected_plan_b {
                                return Err(Error::ErrIncorrectSDPSemantics.into());
                            }
                            if let Some(t) = find_by_mid(mid_value, &mut local_transceivers).await {
                                if let Some(sender) = t.sender().await {
                                    sender.set_negotiated();
                                }
                                let media_transceivers = vec![t];
                                media_sections.push(MediaSection {
                                    id: mid_value.to_owned(),
                                    transceivers: media_transceivers,
                                    rid_map: get_rids(media),
                                    ..Default::default()
                                });
                            } else {
                                return Err(Error::ErrPeerConnTranscieverMidNil.into());
                            }
                        }
                    } else {
                        return Err(Error::ErrPeerConnRemoteDescriptionWithoutMidValue.into());
                    }
                }
            }
        }

        // If we are offering also include unmatched local transceivers
        if include_unmatched {
            if !detected_plan_b {
                for t in &local_transceivers {
                    if let Some(sender) = t.sender().await {
                        sender.set_negotiated();
                    }
                    media_sections.push(MediaSection {
                        id: t.mid().await,
                        transceivers: vec![Arc::clone(t)],
                        ..Default::default()
                    });
                }
            }

            if self
                .sctp_transport
                .data_channels_requested
                .load(Ordering::SeqCst)
                != 0
                && !already_have_application_media_section
            {
                if detected_plan_b {
                    media_sections.push(MediaSection {
                        id: "data".to_owned(),
                        data: true,
                        ..Default::default()
                    });
                } else {
                    media_sections.push(MediaSection {
                        id: format!("{}", media_sections.len()),
                        data: true,
                        ..Default::default()
                    });
                }
            }
        }

        if self.configuration.sdp_semantics == SDPSemantics::UnifiedPlanWithFallback
            && detected_plan_b
        {
            log::info!("Plan-B Offer detected; responding with Plan-B Answer");
        }

        let dtls_fingerprints = vec![];
        /*TODO:dtls_fingerprints, err := self.configuration.Certificates[0].GetFingerprints()
        if err != nil {
            return nil, err
        }*/

        let params = PopulateSdpParams {
            is_plan_b: detected_plan_b,
            media_description_fingerprint: self.setting_engine.sdp_media_level_fingerprints,
            is_icelite: self.setting_engine.candidates.ice_lite,
            connection_role,
            ice_gathering_state: self.ice_gathering_state(),
        };
        populate_sdp(
            d,
            &dtls_fingerprints,
            &self.media_engine,
            &candidates,
            &ice_params,
            &media_sections,
            params,
        )
        .await
    }

    async fn set_gather_complete_handler(&self, f: OnGatheringCompleteHdlrFn) {
        self.ice_gatherer.on_gathering_complete(f).await;
    }

    /// sctp returns the SCTPTransport for this PeerConnection
    ///
    /// The SCTP transport over which SCTP data is sent and received. If SCTP has not been negotiated, the value is nil.
    /// https://www.w3.org/TR/webrtc/#attributes-15
    pub fn sctp(&self) -> Arc<SCTPTransport> {
        Arc::clone(&self.sctp_transport)
    }

    /// gathering_complete_promise is a Pion specific helper function that returns a channel that is closed when gathering is complete.
    /// This function may be helpful in cases where you are unable to trickle your ICE Candidates.
    ///
    /// It is better to not use this function, and instead trickle candidates. If you use this function you will see longer connection startup times.
    /// When the call is connected you will see no impact however.
    pub async fn gathering_complete_promise(&self) -> mpsc::Receiver<()> {
        let (gathering_complete_tx, gathering_complete_rx) = mpsc::channel(1);

        // It's possible to miss the GatherComplete event since setGatherCompleteHandler is an atomic operation and the
        // promise might have been created after the gathering is finished. Therefore, we need to check if the ICE gathering
        // state has changed to complete so that we don't block the caller forever.
        let ice_gatherer = Arc::clone(&self.ice_gatherer);
        let mut done = Some(gathering_complete_tx);
        self.set_gather_complete_handler(Box::new(move || {
            let state = match ice_gatherer.state() {
                ICEGathererState::New => ICEGatheringState::New,
                ICEGathererState::Gathering => ICEGatheringState::Gathering,
                _ => ICEGatheringState::Complete,
            };
            if state == ICEGatheringState::Complete {
                done.take();
            }
            Box::pin(async move {})
        }))
        .await;

        gathering_complete_rx
    }
}
