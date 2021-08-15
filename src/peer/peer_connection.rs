use crate::api::media_engine::MediaEngine;
use crate::api::setting_engine::SettingEngine;
use crate::api::API;
use crate::data::data_channel::DataChannel;
use crate::data::sctp_transport::SCTPTransport;
use crate::media::dtls_transport::dtls_transport_state::DTLSTransportState;
use crate::media::dtls_transport::DTLSTransport;
use crate::media::ice_transport::ice_transport_state::ICETransportState;
use crate::media::ice_transport::ICETransport;
use crate::media::interceptor::Interceptor;
use crate::media::rtp::rtp_receiver::RTPReceiver;
use crate::media::rtp::rtp_transceiver::RTPTransceiver;
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
use crate::peer::sdp::session_description::SessionDescription;
use crate::peer::signaling_state::SignalingState;

use crate::media::rtp::rtp_transceiver_direction::RTPTransceiverDirection;
use crate::peer::operation::Operations;
use crate::peer::sdp::sdp_type::SDPType;
use crate::peer::sdp::{get_by_mid, get_peer_direction, have_data_channel};
use anyhow::Result;
use defer::defer;
use sdp::session_description::ATTR_KEY_MSID;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

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
    signaling_state: AtomicU8,      //SignalingState,
    ice_connection_state: AtomicU8, //ICEConnectionState,
    connection_state: AtomicU8,     //PeerConnectionState,

    idp_login_url: Option<String>,

    is_closed: AtomicBool,              //*atomicBool
    is_negotiation_needed: AtomicBool,  //*atomicBool
    negotiation_needed_state: AtomicU8, //NegotiationNeededState,

    last_offer: String,
    last_answer: String,

    /// a value containing the last known greater mid value
    /// we internally generate mids as numbers. Needed since JSEP
    /// requires that when reusing a media section a new unique mid
    /// should be defined (see JSEP 3.4.1).
    greater_mid: isize,

    rtp_transceivers: Vec<RTPTransceiver>,

    on_signaling_state_change_handler: Arc<Mutex<Option<OnSignalingStateChangeHdlrFn>>>,
    on_connection_state_change_handler: Arc<Mutex<Option<OnPeerConnectionStateChangeHdlrFn>>>,
    on_track_handler: Arc<Mutex<Option<OnTrackHdlrFn>>>,
    on_ice_connection_state_change_handler: Arc<Mutex<Option<OnICEConnectionStateChangeHdlrFn>>>,
    on_data_channel_handler: Arc<Mutex<Option<OnDataChannelHdlrFn>>>,
    on_negotiation_needed_handler: Arc<Mutex<Option<OnNegotiationNeededHdlrFn>>>,

    // interceptorRTCPWriter interceptor.RTCPWriter
    ice_gatherer: Arc<ICEGatherer>,
    ice_transport: Arc<ICETransport>,
    dtls_transport: Arc<DTLSTransport>,
    sctp_transport: Arc<SCTPTransport>,

    // A reference to the associated API state used by this connection
    setting_engine: Arc<SettingEngine>,
    media_engine: Arc<MediaEngine>,
    interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,
}

impl PeerConnection {
    /// creates a PeerConnection with the default codecs and
    /// interceptors.  See register_default_codecs and RegisterDefaultInterceptors.
    ///
    /// If you wish to customize the set of available codecs or the set of
    /// active interceptors, create a MediaEngine and call api.new_peer_connection
    /// instead of this function.
    pub(crate) async fn new(api: &API, configuration: Configuration) -> Result<Arc<Self>> {
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
            is_closed: AtomicBool::new(false),
            is_negotiation_needed: AtomicBool::new(false),
            negotiation_needed_state: AtomicU8::new(NegotiationNeededState::Empty as u8),
            last_offer: "".to_owned(),
            last_answer: "".to_owned(),
            greater_mid: -1,
            signaling_state: AtomicU8::new(SignalingState::Stable as u8),
            ice_connection_state: AtomicU8::new(ICEConnectionState::New as u8),
            connection_state: AtomicU8::new(PeerConnectionState::New as u8),

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

        // Create the ice transport
        pc.ice_transport = Arc::new(api.new_ice_transport(Arc::clone(&pc.ice_gatherer)));

        // Create the DTLS transport
        pc.dtls_transport = Arc::new(api.new_dtls_transport(
            Arc::clone(&pc.ice_transport),
            pc.configuration.certificates.clone(),
        )?);

        // Create the SCTP transport
        pc.sctp_transport = Arc::new(api.new_sctp_transport(Arc::clone(&pc.dtls_transport))?);

        //TODO: pc.interceptorRTCPWriter = api.interceptor.bind_rtcpwriter(interceptor.RTCPWriterFunc(pc.writeRTCP))

        let pc = Arc::new(pc);

        let pc1 = Arc::clone(&pc);
        pc.ice_transport
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
                let pc2 = Arc::clone(&pc1);
                Box::pin(async move {
                    pc2.do_ice_connection_state_change(cs).await;
                    pc2.update_connection_state(cs, pc2.dtls_transport.state())
                        .await;
                })
            }))
            .await;

        // Wire up the on datachannel handler
        let pc1 = Arc::clone(&pc);
        pc.sctp_transport
            .on_data_channel(Box::new(move |d: Arc<DataChannel>| {
                let pc2 = Arc::clone(&pc1);
                Box::pin(async move {
                    let mut handler = pc2.on_data_channel_handler.lock().await;
                    if let Some(f) = &mut *handler {
                        f(d).await;
                    }
                })
            }))
            .await;

        Ok(pc)
    }

    /// init_configuration defines validation of the specified Configuration and
    /// its assignment to the internal configuration variable. This function differs
    /// from its SetConfiguration counterpart because most of the checks do not
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

        /*TODO:
        // https://www.w3.org/TR/webrtc/#constructor (step #3)
        if !configuration.certificates.is_empty() {
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
                let m = get_by_mid(t.mid(), local_desc);
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
                                (m.attribute(ATTR_KEY_MSID), t.sender())
                            {
                                if let Some(track) = &sender.track() {
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
                                    if let Some(rm) = get_by_mid(t.mid(), remote_desc) {
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
                if t.stopped && !t.mid().is_empty() {
                    if let Some(remote_desc) = &self.current_remote_description {
                        if get_by_mid(t.mid(), local_desc).is_some()
                            || get_by_mid(t.mid(), remote_desc).is_some()
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
            if let Some(t) = &t { t.id() } else { "None" }
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

    async fn do_ice_connection_state_change(&self, cs: ICEConnectionState) {
        self.ice_connection_state.store(cs as u8, Ordering::SeqCst);

        log::info!("ICE connection state changed: {}", cs);
        let mut handler = self.on_ice_connection_state_change_handler.lock().await;
        if let Some(f) = &mut *handler {
            f(cs).await;
        }
    }

    /// on_connection_state_change sets an event handler which is called
    /// when the PeerConnectionState has changed
    pub async fn on_connection_state_change(&self, f: OnPeerConnectionStateChangeHdlrFn) {
        let mut on_connection_state_change_handler =
            self.on_connection_state_change_handler.lock().await;
        *on_connection_state_change_handler = Some(f);
    }

    async fn do_connection_state_change(&self, cs: PeerConnectionState) {
        log::info!("Peer connection state changed: {}", cs);
        let mut handler = self.on_connection_state_change_handler.lock().await;
        if let Some(f) = &mut *handler {
            f(cs).await;
        }
    }

    /*
    // SetConfiguration updates the configuration of this PeerConnection object.
    func (pc *PeerConnection) SetConfiguration(configuration Configuration) error { //nolint:gocognit
        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-setconfiguration (step #2)
        if pc.is_closed.get() {
            return &rtcerr.InvalidStateError{Err: ErrConnectionClosed}
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #3)
        if configuration.PeerIdentity != "" {
            if configuration.PeerIdentity != pc.configuration.PeerIdentity {
                return &rtcerr.InvalidModificationError{Err: ErrModifyingPeerIdentity}
            }
            pc.configuration.PeerIdentity = configuration.PeerIdentity
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #4)
        if len(configuration.Certificates) > 0 {
            if len(configuration.Certificates) != len(pc.configuration.Certificates) {
                return &rtcerr.InvalidModificationError{Err: ErrModifyingCertificates}
            }

            for i, certificate := range configuration.Certificates {
                if !pc.configuration.Certificates[i].Equals(certificate) {
                    return &rtcerr.InvalidModificationError{Err: ErrModifyingCertificates}
                }
            }
            pc.configuration.Certificates = configuration.Certificates
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #5)
        if configuration.BundlePolicy != BundlePolicy(Unknown) {
            if configuration.BundlePolicy != pc.configuration.BundlePolicy {
                return &rtcerr.InvalidModificationError{Err: ErrModifyingBundlePolicy}
            }
            pc.configuration.BundlePolicy = configuration.BundlePolicy
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #6)
        if configuration.RTCPMuxPolicy != RTCPMuxPolicy(Unknown) {
            if configuration.RTCPMuxPolicy != pc.configuration.RTCPMuxPolicy {
                return &rtcerr.InvalidModificationError{Err: ErrModifyingRTCPMuxPolicy}
            }
            pc.configuration.RTCPMuxPolicy = configuration.RTCPMuxPolicy
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #7)
        if configuration.ICECandidatePoolSize != 0 {
            if pc.configuration.ICECandidatePoolSize != configuration.ICECandidatePoolSize &&
                pc.LocalDescription() != nil {
                return &rtcerr.InvalidModificationError{Err: ErrModifyingICECandidatePoolSize}
            }
            pc.configuration.ICECandidatePoolSize = configuration.ICECandidatePoolSize
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #8)
        if configuration.ICETransportPolicy != ICETransportPolicy(Unknown) {
            pc.configuration.ICETransportPolicy = configuration.ICETransportPolicy
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #11)
        if len(configuration.ICEServers) > 0 {
            // https://www.w3.org/TR/webrtc/#set-the-configuration (step #11.3)
            for _, server := range configuration.ICEServers {
                if err := server.validate(); err != nil {
                    return err
                }
            }
            pc.configuration.ICEServers = configuration.ICEServers
        }
        return nil
    }

    // GetConfiguration returns a Configuration object representing the current
    // configuration of this PeerConnection object. The returned object is a
    // copy and direct mutation on it will not take affect until SetConfiguration
    // has been called with Configuration passed as its only argument.
    // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-getconfiguration
    func (pc *PeerConnection) GetConfiguration() Configuration {
        return pc.configuration
    }

    func (pc *PeerConnection) getStatsID() string {
        pc.mu.RLock()
        defer pc.mu.RUnlock()
        return pc.stats_id
    }

    // hasLocalDescriptionChanged returns whether local media (rtp_transceivers) has changed
    // caller of this method should hold `pc.mu` lock
    func (pc *PeerConnection) hasLocalDescriptionChanged(desc *SessionDescription) bool {
        for _, t := range pc.rtp_transceivers {
            m := get_by_mid(t.Mid(), desc)
            if m == nil {
                return true
            }

            if get_peer_direction(m) != t.Direction() {
                return true
            }
        }
        return false
    }

    var errExcessiveRetries = errors.New("excessive retries in CreateOffer")

    // CreateOffer starts the PeerConnection and generates the localDescription
    // https://w3c.github.io/webrtc-pc/#dom-rtcpeerconnection-createoffer
    func (pc *PeerConnection) CreateOffer(options *OfferOptions) (SessionDescription, error) { //nolint:gocognit
        useIdentity := pc.idp_login_url != nil
        switch {
        case useIdentity:
            return SessionDescription{}, errIdentityProviderNotImplemented
        case pc.is_closed.get():
            return SessionDescription{}, &rtcerr.InvalidStateError{Err: ErrConnectionClosed}
        }

        if options != nil && options.ICERestart {
            if err := pc.iceTransport.restart(); err != nil {
                return SessionDescription{}, err
            }
        }

        var (
            d     *sdp.SessionDescription
            offer SessionDescription
            err   error
        )

        // This may be necessary to recompute if, for example, createOffer was called when only an
        // audio RTCRtpTransceiver was added to connection, but while performing the in-parallel
        // steps to create an offer, a video RTCRtpTransceiver was added, requiring additional
        // inspection of video system resources.
        count := 0
        pc.mu.Lock()
        defer pc.mu.Unlock()
        for {
            // We cache current transceivers to ensure they aren't
            // mutated during offer generation. We later check if they have
            // been mutated and recompute the offer if necessary.
            currentTransceivers := pc.rtp_transceivers

            // in-parallel steps to create an offer
            // https://w3c.github.io/webrtc-pc/#dfn-in-parallel-steps-to-create-an-offer
            isPlanB := pc.configuration.SDPSemantics == SDPSemanticsPlanB
            if pc.current_remote_description != nil {
                isPlanB = description_is_plan_b(pc.current_remote_description)
            }

            // include unmatched local transceivers
            if !isPlanB {
                // update the greater mid if the remote description provides a greater one
                if pc.current_remote_description != nil {
                    var numericMid int
                    for _, media := range pc.current_remote_description.parsed.MediaDescriptions {
                        mid := getMidValue(media)
                        if mid == "" {
                            continue
                        }
                        numericMid, err = strconv.Atoi(mid)
                        if err != nil {
                            continue
                        }
                        if numericMid > pc.greater_mid {
                            pc.greater_mid = numericMid
                        }
                    }
                }
                for _, t := range currentTransceivers {
                    if t.Mid() != "" {
                        continue
                    }
                    pc.greater_mid++
                    err = t.setMid(strconv.Itoa(pc.greater_mid))
                    if err != nil {
                        return SessionDescription{}, err
                    }
                }
            }

            if pc.current_remote_description == nil {
                d, err = pc.generateUnmatchedSDP(currentTransceivers, useIdentity)
            } else {
                d, err = pc.generateMatchedSDP(currentTransceivers, useIdentity, true /*includeUnmatched */, connectionRoleFromDtlsRole(defaultDtlsRoleOffer))
            }

            if err != nil {
                return SessionDescription{}, err
            }

            update_sdp_origin(&pc.sdp_origin, d)
            sdpBytes, err := d.Marshal()
            if err != nil {
                return SessionDescription{}, err
            }

            offer = SessionDescription{
                Type:   SDPTypeOffer,
                SDP:    string(sdpBytes),
                parsed: d,
            }

            // Verify local media hasn't changed during offer
            // generation. Recompute if necessary
            if isPlanB || !pc.hasLocalDescriptionChanged(&offer) {
                break
            }
            count++
            if count >= 128 {
                return SessionDescription{}, errExcessiveRetries
            }
        }

        pc.last_offer = offer.SDP
        return offer, nil
    }
    */

    /// Update the PeerConnectionState given the state of relevant transports
    /// https://www.w3.org/TR/webrtc/#rtcpeerconnectionstate-enum
    async fn update_connection_state(
        &self,
        ice_connection_state: ICEConnectionState,
        dtls_transport_state: DTLSTransportState,
    ) {
        let  connection_state =
        // The RTCPeerConnection object's [[IsClosed]] slot is true.
        if self.is_closed.load(Ordering::SeqCst) {
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

        if self.connection_state.load(Ordering::SeqCst) == connection_state as u8 {
            return;
        }

        log::info!("peer connection state changed: {}", connection_state);
        self.connection_state
            .store(connection_state as u8, Ordering::SeqCst);

        self.do_connection_state_change(connection_state).await;
    }

    /*
        // CreateAnswer starts the PeerConnection and generates the localDescription
        func (pc *PeerConnection) CreateAnswer(options *AnswerOptions) (SessionDescription, error) {
            useIdentity := pc.idp_login_url != nil
            switch {
            case pc.RemoteDescription() == nil:
                return SessionDescription{}, &rtcerr.InvalidStateError{Err: ErrNoRemoteDescription}
            case useIdentity:
                return SessionDescription{}, errIdentityProviderNotImplemented
            case pc.is_closed.get():
                return SessionDescription{}, &rtcerr.InvalidStateError{Err: ErrConnectionClosed}
            case pc.signaling_state.Get() != SignalingStateHaveRemoteOffer && pc.signaling_state.Get() != SignalingStateHaveLocalPranswer:
                return SessionDescription{}, &rtcerr.InvalidStateError{Err: ErrIncorrectSignalingState}
            }

            connectionRole := connectionRoleFromDtlsRole(pc.api.settingEngine.answeringDTLSRole)
            if connectionRole == sdp.ConnectionRole(0) {
                connectionRole = connectionRoleFromDtlsRole(defaultDtlsRoleAnswer)
            }
            pc.mu.Lock()
            defer pc.mu.Unlock()

            d, err := pc.generateMatchedSDP(pc.rtp_transceivers, useIdentity, false /*includeUnmatched */, connectionRole)
            if err != nil {
                return SessionDescription{}, err
            }

            update_sdporigin(&pc.sdp_origin, d)
            sdpBytes, err := d.Marshal()
            if err != nil {
                return SessionDescription{}, err
            }

            desc := SessionDescription{
                Type:   SDPTypeAnswer,
                SDP:    string(sdpBytes),
                parsed: d,
            }
            pc.last_answer = desc.SDP
            return desc, nil
        }

        // 4.4.1.6 Set the SessionDescription
        func (pc *PeerConnection) setDescription(sd *SessionDescription, op stateChangeOp) error { //nolint:gocognit
            switch {
            case pc.is_closed.get():
                return &rtcerr.InvalidStateError{Err: ErrConnectionClosed}
            case NewSDPType(sd.Type.String()) == SDPType(Unknown):
                return &rtcerr.TypeError{Err: fmt.Errorf("%w: '%d' is not a valid enum value of type SDPType", errPeerConnSDPTypeInvalidValue, sd.Type)}
            }

            nextState, err := func() (signaling_state, error) {
                pc.mu.Lock()
                defer pc.mu.Unlock()

                cur := pc.signaling_state()
                setLocal := stateChangeOpSetLocal
                setRemote := stateChangeOpSetRemote
                newSDPDoesNotMatchOffer := &rtcerr.InvalidModificationError{Err: errSDPDoesNotMatchOffer}
                newSDPDoesNotMatchAnswer := &rtcerr.InvalidModificationError{Err: errSDPDoesNotMatchAnswer}

                var nextState signaling_state
                var err error
                switch op {
                case setLocal:
                    switch sd.Type {
                    // stable->SetLocal(offer)->have-local-offer
                    case SDPTypeOffer:
                        if sd.SDP != pc.last_offer {
                            return nextState, newSDPDoesNotMatchOffer
                        }
                        nextState, err = checkNextSignalingState(cur, SignalingStateHaveLocalOffer, setLocal, sd.Type)
                        if err == nil {
                            pc.pending_local_description = sd
                        }
                    // have-remote-offer->SetLocal(answer)->stable
                    // have-local-pranswer->SetLocal(answer)->stable
                    case SDPTypeAnswer:
                        if sd.SDP != pc.last_answer {
                            return nextState, newSDPDoesNotMatchAnswer
                        }
                        nextState, err = checkNextSignalingState(cur, SignalingStateStable, setLocal, sd.Type)
                        if err == nil {
                            pc.current_local_description = sd
                            pc.current_remote_description = pc.pending_remote_description
                            pc.pending_remote_description = nil
                            pc.pending_local_description = nil
                        }
                    case SDPTypeRollback:
                        nextState, err = checkNextSignalingState(cur, SignalingStateStable, setLocal, sd.Type)
                        if err == nil {
                            pc.pending_local_description = nil
                        }
                    // have-remote-offer->SetLocal(pranswer)->have-local-pranswer
                    case SDPTypePranswer:
                        if sd.SDP != pc.last_answer {
                            return nextState, newSDPDoesNotMatchAnswer
                        }
                        nextState, err = checkNextSignalingState(cur, SignalingStateHaveLocalPranswer, setLocal, sd.Type)
                        if err == nil {
                            pc.pending_local_description = sd
                        }
                    default:
                        return nextState, &rtcerr.OperationError{Err: fmt.Errorf("%w: %s(%s)", errPeerConnStateChangeInvalid, op, sd.Type)}
                    }
                case setRemote:
                    switch sd.Type {
                    // stable->SetRemote(offer)->have-remote-offer
                    case SDPTypeOffer:
                        nextState, err = checkNextSignalingState(cur, SignalingStateHaveRemoteOffer, setRemote, sd.Type)
                        if err == nil {
                            pc.pending_remote_description = sd
                        }
                    // have-local-offer->SetRemote(answer)->stable
                    // have-remote-pranswer->SetRemote(answer)->stable
                    case SDPTypeAnswer:
                        nextState, err = checkNextSignalingState(cur, SignalingStateStable, setRemote, sd.Type)
                        if err == nil {
                            pc.current_remote_description = sd
                            pc.current_local_description = pc.pending_local_description
                            pc.pending_remote_description = nil
                            pc.pending_local_description = nil
                        }
                    case SDPTypeRollback:
                        nextState, err = checkNextSignalingState(cur, SignalingStateStable, setRemote, sd.Type)
                        if err == nil {
                            pc.pending_remote_description = nil
                        }
                    // have-local-offer->SetRemote(pranswer)->have-remote-pranswer
                    case SDPTypePranswer:
                        nextState, err = checkNextSignalingState(cur, SignalingStateHaveRemotePranswer, setRemote, sd.Type)
                        if err == nil {
                            pc.pending_remote_description = sd
                        }
                    default:
                        return nextState, &rtcerr.OperationError{Err: fmt.Errorf("%w: %s(%s)", errPeerConnStateChangeInvalid, op, sd.Type)}
                    }
                default:
                    return nextState, &rtcerr.OperationError{Err: fmt.Errorf("%w: %q", errPeerConnStateChangeUnhandled, op)}
                }

                return nextState, err
            }()

            if err == nil {
                pc.signaling_state.Set(nextState)
                if pc.signaling_state.Get() == SignalingStateStable {
                    pc.is_negotiation_needed.set(false)
                    pc.mu.Lock()
                    pc.onNegotiationNeeded()
                    pc.mu.Unlock()
                }
                pc.onSignalingStateChange(nextState)
            }
            return err
        }

        // SetLocalDescription sets the SessionDescription of the local peer
        func (pc *PeerConnection) SetLocalDescription(desc SessionDescription) error {
            if pc.is_closed.get() {
                return &rtcerr.InvalidStateError{Err: ErrConnectionClosed}
            }

            haveLocalDescription := pc.current_local_description != nil

            // JSEP 5.4
            if desc.SDP == "" {
                switch desc.Type {
                case SDPTypeAnswer, SDPTypePranswer:
                    desc.SDP = pc.last_answer
                case SDPTypeOffer:
                    desc.SDP = pc.last_offer
                default:
                    return &rtcerr.InvalidModificationError{
                        Err: fmt.Errorf("%w: %s", errPeerConnSDPTypeInvalidValueSetLocalDescription, desc.Type),
                    }
                }
            }

            desc.parsed = &sdp.SessionDescription{}
            if err := desc.parsed.Unmarshal([]byte(desc.SDP)); err != nil {
                return err
            }
            if err := pc.setDescription(&desc, stateChangeOpSetLocal); err != nil {
                return err
            }

            currentTransceivers := append([]*RTPTransceiver{}, pc.GetTransceivers()...)

            weAnswer := desc.Type == SDPTypeAnswer
            remoteDesc := pc.RemoteDescription()
            if weAnswer && remoteDesc != nil {
                if err := pc.startRTPSenders(currentTransceivers); err != nil {
                    return err
                }
                pc.ops.Enqueue(func() {
                    pc.startRTP(haveLocalDescription, remoteDesc, currentTransceivers)
                })
            }

            if pc.iceGatherer.State() == ICEGathererStateNew {
                return pc.iceGatherer.Gather()
            }
            return nil
        }

        // LocalDescription returns PendingLocalDescription if it is not null and
        // otherwise it returns CurrentLocalDescription. This property is used to
        // determine if SetLocalDescription has already been called.
        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-localdescription
        func (pc *PeerConnection) LocalDescription() *SessionDescription {
            if pending_local_description := pc.PendingLocalDescription(); pending_local_description != nil {
                return pending_local_description
            }
            return pc.CurrentLocalDescription()
        }

        // SetRemoteDescription sets the SessionDescription of the remote peer
        // nolint: gocyclo
        func (pc *PeerConnection) SetRemoteDescription(desc SessionDescription) error { //nolint:gocognit
            if pc.is_closed.get() {
                return &rtcerr.InvalidStateError{Err: ErrConnectionClosed}
            }

            isRenegotation := pc.current_remote_description != nil

            if _, err := desc.Unmarshal(); err != nil {
                return err
            }
            if err := pc.setDescription(&desc, stateChangeOpSetRemote); err != nil {
                return err
            }

            if err := pc.api.mediaEngine.updateFromRemoteDescription(*desc.parsed); err != nil {
                return err
            }

            var t *RTPTransceiver
            localTransceivers := append([]*RTPTransceiver{}, pc.GetTransceivers()...)
            detectedPlanB := description_is_plan_b(pc.RemoteDescription())
            weOffer := desc.Type == SDPTypeAnswer

            if !weOffer && !detectedPlanB {
                for _, media := range pc.RemoteDescription().parsed.MediaDescriptions {
                    midValue := getMidValue(media)
                    if midValue == "" {
                        return errPeerConnRemoteDescriptionWithoutMidValue
                    }

                    if media.MediaName.Media == mediaSectionApplication {
                        continue
                    }

                    kind := NewRTPCodecType(media.MediaName.Media)
                    direction := get_peer_direction(media)
                    if kind == 0 || direction == RTPTransceiverDirection(Unknown) {
                        continue
                    }

                    t, localTransceivers = findByMid(midValue, localTransceivers)
                    if t == nil {
                        t, localTransceivers = satisfyTypeAndDirection(kind, direction, localTransceivers)
                    } else if direction == RTPTransceiverDirectionInactive {
                        if err := t.Stop(); err != nil {
                            return err
                        }
                    }

                    switch {
                    case t == nil:
                        receiver, err := pc.api.new_rtpreceiver(kind, pc.dtlsTransport)
                        if err != nil {
                            return err
                        }

                        localDirection := RTPTransceiverDirectionRecvonly
                        if direction == RTPTransceiverDirectionRecvonly {
                            localDirection = RTPTransceiverDirectionSendonly
                        }

                        t = newRTPTransceiver(receiver, nil, localDirection, kind)
                        pc.mu.Lock()
                        pc.addRTPTransceiver(t)
                        pc.mu.Unlock()
                    case direction == RTPTransceiverDirectionRecvonly:
                        if t.Direction() == RTPTransceiverDirectionSendrecv {
                            t.setDirection(RTPTransceiverDirectionSendonly)
                        }
                    case direction == RTPTransceiverDirectionSendrecv:
                        if t.Direction() == RTPTransceiverDirectionSendonly {
                            t.setDirection(RTPTransceiverDirectionSendrecv)
                        }
                    }

                    if t.Mid() == "" {
                        if err := t.setMid(midValue); err != nil {
                            return err
                        }
                    }
                }
            }

            remoteUfrag, remotePwd, candidates, err := extract_icedetails(desc.parsed)
            if err != nil {
                return err
            }

            if isRenegotation && pc.iceTransport.haveRemoteCredentialsChange(remoteUfrag, remotePwd) {
                // An ICE Restart only happens implicitly for a SetRemoteDescription of type offer
                if !weOffer {
                    if err = pc.iceTransport.restart(); err != nil {
                        return err
                    }
                }

                if err = pc.iceTransport.setRemoteCredentials(remoteUfrag, remotePwd); err != nil {
                    return err
                }
            }

            for i := range candidates {
                if err = pc.iceTransport.AddRemoteCandidate(&candidates[i]); err != nil {
                    return err
                }
            }

            currentTransceivers := append([]*RTPTransceiver{}, pc.GetTransceivers()...)

            if isRenegotation {
                if weOffer {
                    if err = pc.startRTPSenders(currentTransceivers); err != nil {
                        return err
                    }
                    pc.ops.Enqueue(func() {
                        pc.startRTP(true, &desc, currentTransceivers)
                    })
                }
                return nil
            }

            remoteIsLite := false
            for _, a := range desc.parsed.Attributes {
                if strings.TrimSpace(a.Key) == sdp.AttrKeyICELite {
                    remoteIsLite = true
                }
            }

            fingerprint, fingerprintHash, err := extract_fingerprint(desc.parsed)
            if err != nil {
                return err
            }

            iceRole := ICERoleControlled
            // If one of the agents is lite and the other one is not, the lite agent must be the controlling agent.
            // If both or neither agents are lite the offering agent is controlling.
            // RFC 8445 S6.1.1
            if (weOffer && remoteIsLite == pc.api.settingEngine.candidates.ICELite) || (remoteIsLite && !pc.api.settingEngine.candidates.ICELite) {
                iceRole = ICERoleControlling
            }

            // Start the networking in a new routine since it will block until
            // the connection is actually established.
            if weOffer {
                if err := pc.startRTPSenders(currentTransceivers); err != nil {
                    return err
                }
            }

            pc.ops.Enqueue(func() {
                pc.startTransports(iceRole, dtlsRoleFromRemoteSDP(desc.parsed), remoteUfrag, remotePwd, fingerprint, fingerprintHash)
                if weOffer {
                    pc.startRTP(false, &desc, currentTransceivers)
                }
            })
            return nil
        }

        func (pc *PeerConnection) startReceiver(incoming TrackDetails, receiver *RTPReceiver) {
            encodings := []RTPDecodingParameters{}
            if incoming.ssrc != 0 {
                encodings = append(encodings, RTPDecodingParameters{RTPCodingParameters{SSRC: incoming.ssrc}})
            }
            for _, rid := range incoming.rids {
                encodings = append(encodings, RTPDecodingParameters{RTPCodingParameters{RID: rid}})
            }

            if err := receiver.Receive(RTPReceiveParameters{Encodings: encodings}); err != nil {
                pc.log.Warnf("RTPReceiver Receive failed %s", err)
                return
            }

            // set track id and label early so they can be set as new track information
            // is received from the SDP.
            for i := range receiver.tracks {
                receiver.tracks[i].track.mu.Lock()
                receiver.tracks[i].track.id = incoming.id
                receiver.tracks[i].track.streamID = incoming.streamID
                receiver.tracks[i].track.mu.Unlock()
            }

            // We can't block and wait for a single SSRC
            if incoming.ssrc == 0 {
                return
            }

            go func() {
                if err := receiver.Track().determinePayloadType(); err != nil {
                    pc.log.Warnf("Could not determine PayloadType for SSRC %d", receiver.Track().SSRC())
                    return
                }

                params, err := pc.api.mediaEngine.getRTPParametersByPayloadType(receiver.Track().PayloadType())
                if err != nil {
                    pc.log.Warnf("no codec could be found for payloadType %d", receiver.Track().PayloadType())
                    return
                }

                receiver.Track().mu.Lock()
                receiver.Track().kind = receiver.kind
                receiver.Track().codec = params.Codecs[0]
                receiver.Track().params = params
                receiver.Track().mu.Unlock()

                pc.onTrack(receiver.Track(), receiver)
            }()
        }

        // startRTPReceivers opens knows inbound SRTP streams from the RemoteDescription
        func (pc *PeerConnection) startRTPReceivers(incomingTracks []TrackDetails, currentTransceivers []*RTPTransceiver) { //nolint:gocognit
            localTransceivers := append([]*RTPTransceiver{}, currentTransceivers...)

            remoteIsPlanB := false
            switch pc.configuration.SDPSemantics {
            case SDPSemanticsPlanB:
                remoteIsPlanB = true
            case SDPSemanticsUnifiedPlanWithFallback:
                remoteIsPlanB = description_is_plan_b(pc.RemoteDescription())
            default:
                // none
            }

            // Ensure we haven't already started a transceiver for this ssrc
            for i := range incomingTracks {
                if len(incomingTracks) <= i {
                    break
                }
                incomingTrack := incomingTracks[i]

                for _, t := range localTransceivers {
                    if (t.Receiver()) == nil || t.Receiver().Track() == nil || t.Receiver().Track().ssrc != incomingTrack.ssrc {
                        continue
                    }

                    incomingTracks = filter_track_with_ssrc(incomingTracks, incomingTrack.ssrc)
                }
            }

            unhandledTracks := incomingTracks[:0]
            for i := range incomingTracks {
                trackHandled := false
                for j := range localTransceivers {
                    t := localTransceivers[j]
                    incomingTrack := incomingTracks[i]

                    if t.Mid() != incomingTrack.mid {
                        continue
                    }

                    if (incomingTrack.kind != t.kind) ||
                        (t.Direction() != RTPTransceiverDirectionRecvonly && t.Direction() != RTPTransceiverDirectionSendrecv) ||
                        (t.Receiver()) == nil ||
                        (t.Receiver().haveReceived()) {
                        continue
                    }

                    pc.startReceiver(incomingTrack, t.Receiver())
                    trackHandled = true
                    break
                }

                if !trackHandled {
                    unhandledTracks = append(unhandledTracks, incomingTracks[i])
                }
            }

            if remoteIsPlanB {
                for _, incoming := range unhandledTracks {
                    t, err := pc.AddTransceiverFromKind(incoming.kind, RTPTransceiverInit{
                        Direction: RTPTransceiverDirectionSendrecv,
                    })
                    if err != nil {
                        pc.log.Warnf("Could not add transceiver for remote SSRC %d: %s", incoming.ssrc, err)
                        continue
                    }
                    pc.startReceiver(incoming, t.Receiver())
                }
            }
        }

        // startRTPSenders starts all outbound RTP streams
        func (pc *PeerConnection) startRTPSenders(currentTransceivers []*RTPTransceiver) error {
            for _, transceiver := range currentTransceivers {
                if transceiver.Sender() != nil && transceiver.Sender().isNegotiated() && !transceiver.Sender().hasSent() {
                    err := transceiver.Sender().Send(RTPSendParameters{
                        Encodings: []RTPEncodingParameters{
                            {
                                RTPCodingParameters{
                                    SSRC:        transceiver.Sender().ssrc,
                                    PayloadType: transceiver.Sender().payloadType,
                                },
                            },
                        },
                    })
                    if err != nil {
                        return err
                    }
                }
            }

            return nil
        }

        // Start SCTP subsystem
        func (pc *PeerConnection) startSCTP() {
            // Start sctp
            if err := pc.sctpTransport.Start(SCTPCapabilities{
                MaxMessageSize: 0,
            }); err != nil {
                pc.log.Warnf("Failed to start SCTP: %s", err)
                if err = pc.sctpTransport.Stop(); err != nil {
                    pc.log.Warnf("Failed to stop SCTPTransport: %s", err)
                }

                return
            }

            // DataChannels that need to be opened now that SCTP is available
            // make a copy we may have incoming DataChannels mutating this while we open
            pc.sctpTransport.lock.RLock()
            dataChannels := append([]*DataChannel{}, pc.sctpTransport.dataChannels...)
            pc.sctpTransport.lock.RUnlock()

            var openedDCCount uint32
            for _, d := range dataChannels {
                if d.ReadyState() == DataChannelStateConnecting {
                    err := d.open(pc.sctpTransport)
                    if err != nil {
                        pc.log.Warnf("failed to open data channel: %s", err)
                        continue
                    }
                    openedDCCount++
                }
            }

            pc.sctpTransport.lock.Lock()
            pc.sctpTransport.dataChannelsOpened += openedDCCount
            pc.sctpTransport.lock.Unlock()
        }

        func (pc *PeerConnection) handleUndeclaredSSRC(rtpStream io.Reader, ssrc SSRC) error { //nolint:gocognit
            remoteDescription := pc.RemoteDescription()
            if remoteDescription == nil {
                return errPeerConnRemoteDescriptionNil
            }

            // If the remote SDP was only one media section the ssrc doesn't have to be explicitly declared
            if len(remoteDescription.parsed.MediaDescriptions) == 1 {
                onlyMediaSection := remoteDescription.parsed.MediaDescriptions[0]
                for _, a := range onlyMediaSection.Attributes {
                    if a.Key == ssrcStr {
                        return errPeerConnSingleMediaSectionHasExplicitSSRC
                    }
                }

                incoming := TrackDetails{
                    ssrc: ssrc,
                    kind: RTPCodecTypeVideo,
                }
                if onlyMediaSection.MediaName.Media == RTPCodecTypeAudio.String() {
                    incoming.kind = RTPCodecTypeAudio
                }

                t, err := pc.AddTransceiverFromKind(incoming.kind, RTPTransceiverInit{
                    Direction: RTPTransceiverDirectionSendrecv,
                })
                if err != nil {
                    return fmt.Errorf("%w: %d: %s", errPeerConnRemoteSSRCAddTransceiver, ssrc, err)
                }
                pc.startReceiver(incoming, t.Receiver())
                return nil
            }

            midExtensionID, audioSupported, videoSupported := pc.api.mediaEngine.getHeaderExtensionID(RTPHeaderExtensionCapability{sdp.SDESMidURI})
            if !audioSupported && !videoSupported {
                return errPeerConnSimulcastMidRTPExtensionRequired
            }

            streamIDExtensionID, audioSupported, videoSupported := pc.api.mediaEngine.getHeaderExtensionID(RTPHeaderExtensionCapability{sdp.SDESRTPStreamIDURI})
            if !audioSupported && !videoSupported {
                return errPeerConnSimulcastStreamIDRTPExtensionRequired
            }

            b := make([]byte, receiveMTU)
            var mid, rid string
            for readCount := 0; readCount <= simulcastProbeCount; readCount++ {
                i, err := rtpStream.Read(b)
                if err != nil {
                    return err
                }

                maybeMid, maybeRid, payloadType, err := handleUnknownRTPPacket(b[:i], uint8(midExtensionID), uint8(streamIDExtensionID))
                if err != nil {
                    return err
                }

                if maybeMid != "" {
                    mid = maybeMid
                }
                if maybeRid != "" {
                    rid = maybeRid
                }

                if mid == "" || rid == "" {
                    continue
                }

                params, err := pc.api.mediaEngine.getRTPParametersByPayloadType(payloadType)
                if err != nil {
                    return err
                }

                for _, t := range pc.GetTransceivers() {
                    if t.Mid() != mid || t.Receiver() == nil {
                        continue
                    }

                    track, err := t.Receiver().receiveForRid(rid, params, ssrc)
                    if err != nil {
                        return err
                    }
                    pc.onTrack(track, t.Receiver())
                    return nil
                }
            }

            return errPeerConnSimulcastIncomingSSRCFailed
        }

        // undeclaredMediaProcessor handles RTP/RTCP packets that don't match any a:ssrc lines
        func (pc *PeerConnection) undeclaredMediaProcessor() {
            go func() {
                var simulcastRoutineCount uint64
                for {
                    srtpSession, err := pc.dtlsTransport.getSRTPSession()
                    if err != nil {
                        pc.log.Warnf("undeclaredMediaProcessor failed to open SrtpSession: %v", err)
                        return
                    }

                    stream, ssrc, err := srtpSession.AcceptStream()
                    if err != nil {
                        pc.log.Warnf("Failed to accept RTP %v", err)
                        return
                    }

                    if pc.is_closed.get() {
                        if err = stream.Close(); err != nil {
                            pc.log.Warnf("Failed to close RTP stream %v", err)
                        }
                        continue
                    }

                    if atomic.AddUint64(&simulcastRoutineCount, 1) >= simulcastMaxProbeRoutines {
                        atomic.AddUint64(&simulcastRoutineCount, ^uint64(0))
                        pc.log.Warn(ErrSimulcastProbeOverflow.Error())
                        continue
                    }

                    go func(rtpStream io.Reader, ssrc SSRC) {
                        pc.dtlsTransport.storeSimulcastStream(stream)

                        if err := pc.handleUndeclaredSSRC(rtpStream, ssrc); err != nil {
                            pc.log.Errorf("Incoming unhandled RTP ssrc(%d), on_track will not be fired. %v", ssrc, err)
                        }
                        atomic.AddUint64(&simulcastRoutineCount, ^uint64(0))
                    }(stream, SSRC(ssrc))
                }
            }()

            go func() {
                for {
                    srtcpSession, err := pc.dtlsTransport.getSRTCPSession()
                    if err != nil {
                        pc.log.Warnf("undeclaredMediaProcessor failed to open SrtcpSession: %v", err)
                        return
                    }

                    _, ssrc, err := srtcpSession.AcceptStream()
                    if err != nil {
                        pc.log.Warnf("Failed to accept RTCP %v", err)
                        return
                    }
                    pc.log.Warnf("Incoming unhandled RTCP ssrc(%d), on_track will not be fired", ssrc)
                }
            }()
        }

        // RemoteDescription returns pending_remote_description if it is not null and
        // otherwise it returns current_remote_description. This property is used to
        // determine if setRemoteDescription has already been called.
        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-remotedescription
        func (pc *PeerConnection) RemoteDescription() *SessionDescription {
            pc.mu.RLock()
            defer pc.mu.RUnlock()

            if pc.pending_remote_description != nil {
                return pc.pending_remote_description
            }
            return pc.current_remote_description
        }

        // AddICECandidate accepts an ICE candidate string and adds it
        // to the existing set of candidates.
        func (pc *PeerConnection) AddICECandidate(candidate ICECandidateInit) error {
            if pc.RemoteDescription() == nil {
                return &rtcerr.InvalidStateError{Err: ErrNoRemoteDescription}
            }

            candidateValue := strings.TrimPrefix(candidate.Candidate, "candidate:")

            var iceCandidate *ICECandidate
            if candidateValue != "" {
                candidate, err := ice.UnmarshalCandidate(candidateValue)
                if err != nil {
                    return err
                }

                c, err := newICECandidateFromICE(candidate)
                if err != nil {
                    return err
                }
                iceCandidate = &c
            }

            return pc.iceTransport.AddRemoteCandidate(iceCandidate)
        }

        // ICEConnectionState returns the ICE connection state of the
        // PeerConnection instance.
        func (pc *PeerConnection) ICEConnectionState() ICEConnectionState {
            pc.mu.RLock()
            defer pc.mu.RUnlock()

            return pc.ice_connection_state
        }

        // GetSenders returns the RTPSender that are currently attached to this PeerConnection
        func (pc *PeerConnection) GetSenders() (result []*RTPSender) {
            pc.mu.Lock()
            defer pc.mu.Unlock()

            for _, transceiver := range pc.rtp_transceivers {
                if transceiver.Sender() != nil {
                    result = append(result, transceiver.Sender())
                }
            }
            return result
        }

        // GetReceivers returns the RTPReceivers that are currently attached to this PeerConnection
        func (pc *PeerConnection) GetReceivers() (receivers []*RTPReceiver) {
            pc.mu.Lock()
            defer pc.mu.Unlock()

            for _, transceiver := range pc.rtp_transceivers {
                if transceiver.Receiver() != nil {
                    receivers = append(receivers, transceiver.Receiver())
                }
            }
            return
        }

        // GetTransceivers returns the RtpTransceiver that are currently attached to this PeerConnection
        func (pc *PeerConnection) GetTransceivers() []*RTPTransceiver {
            pc.mu.Lock()
            defer pc.mu.Unlock()

            return pc.rtp_transceivers
        }

        // AddTrack adds a Track to the PeerConnection
        func (pc *PeerConnection) AddTrack(track TrackLocal) (*RTPSender, error) {
            if pc.is_closed.get() {
                return nil, &rtcerr.InvalidStateError{Err: ErrConnectionClosed}
            }

            pc.mu.Lock()
            defer pc.mu.Unlock()
            for _, t := range pc.rtp_transceivers {
                if !t.stopped && t.kind == track.kind() && t.Sender() == nil {
                    sender, err := pc.api.new_rtpsender(track, pc.dtlsTransport)
                    if err == nil {
                        err = t.SetSender(sender, track)
                        if err != nil {
                            _ = sender.Stop()
                            t.setSender(nil)
                        }
                    }
                    if err != nil {
                        return nil, err
                    }
                    pc.onNegotiationNeeded()
                    return sender, nil
                }
            }

            transceiver, err := pc.newTransceiverFromTrack(RTPTransceiverDirectionSendrecv, track)
            if err != nil {
                return nil, err
            }
            pc.addRTPTransceiver(transceiver)
            return transceiver.Sender(), nil
        }

        // RemoveTrack removes a Track from the PeerConnection
        func (pc *PeerConnection) RemoveTrack(sender *RTPSender) (err error) {
            if pc.is_closed.get() {
                return &rtcerr.InvalidStateError{Err: ErrConnectionClosed}
            }

            var transceiver *RTPTransceiver
            pc.mu.Lock()
            defer pc.mu.Unlock()
            for _, t := range pc.rtp_transceivers {
                if t.Sender() == sender {
                    transceiver = t
                    break
                }
            }
            if transceiver == nil {
                return &rtcerr.InvalidAccessError{Err: ErrSenderNotCreatedByConnection}
            } else if err = sender.Stop(); err == nil {
                err = transceiver.setSendingTrack(nil)
                if err == nil {
                    pc.onNegotiationNeeded()
                }
            }
            return
        }

        func (pc *PeerConnection) newTransceiverFromTrack(direction RTPTransceiverDirection, track TrackLocal) (t *RTPTransceiver, err error) {
            var (
                r *RTPReceiver
                s *RTPSender
            )
            switch direction {
            case RTPTransceiverDirectionSendrecv:
                r, err = pc.api.new_rtpreceiver(track.kind(), pc.dtlsTransport)
                if err != nil {
                    return
                }
                s, err = pc.api.new_rtpsender(track, pc.dtlsTransport)
            case RTPTransceiverDirectionSendonly:
                s, err = pc.api.new_rtpsender(track, pc.dtlsTransport)
            default:
                err = errPeerConnAddTransceiverFromTrackSupport
            }
            if err != nil {
                return
            }
            return newRTPTransceiver(r, s, direction, track.kind()), nil
        }

        // AddTransceiverFromKind Create a new RtpTransceiver and adds it to the set of transceivers.
        func (pc *PeerConnection) AddTransceiverFromKind(kind RTPCodecType, init ...RTPTransceiverInit) (t *RTPTransceiver, err error) {
            if pc.is_closed.get() {
                return nil, &rtcerr.InvalidStateError{Err: ErrConnectionClosed}
            }

            direction := RTPTransceiverDirectionSendrecv
            if len(init) > 1 {
                return nil, errPeerConnAddTransceiverFromKindOnlyAcceptsOne
            } else if len(init) == 1 {
                direction = init[0].Direction
            }
            switch direction {
            case RTPTransceiverDirectionSendonly, RTPTransceiverDirectionSendrecv:
                codecs := pc.api.mediaEngine.getCodecsByKind(kind)
                if len(codecs) == 0 {
                    return nil, ErrNoCodecsAvailable
                }
                track, err := NewTrackLocalStaticSample(codecs[0].RTPCodecCapability, util.MathRandAlpha(16), util.MathRandAlpha(16))
                if err != nil {
                    return nil, err
                }
                t, err = pc.newTransceiverFromTrack(direction, track)
                if err != nil {
                    return nil, err
                }
            case RTPTransceiverDirectionRecvonly:
                receiver, err := pc.api.new_rtpreceiver(kind, pc.dtlsTransport)
                if err != nil {
                    return nil, err
                }
                t = newRTPTransceiver(receiver, nil, RTPTransceiverDirectionRecvonly, kind)
            default:
                return nil, errPeerConnAddTransceiverFromKindSupport
            }
            pc.mu.Lock()
            pc.addRTPTransceiver(t)
            pc.mu.Unlock()
            return t, nil
        }

        // AddTransceiverFromTrack Create a new RtpTransceiver(SendRecv or SendOnly) and add it to the set of transceivers.
        func (pc *PeerConnection) AddTransceiverFromTrack(track TrackLocal, init ...RTPTransceiverInit) (t *RTPTransceiver, err error) {
            if pc.is_closed.get() {
                return nil, &rtcerr.InvalidStateError{Err: ErrConnectionClosed}
            }

            direction := RTPTransceiverDirectionSendrecv
            if len(init) > 1 {
                return nil, errPeerConnAddTransceiverFromTrackOnlyAcceptsOne
            } else if len(init) == 1 {
                direction = init[0].Direction
            }

            t, err = pc.newTransceiverFromTrack(direction, track)
            if err == nil {
                pc.mu.Lock()
                pc.addRTPTransceiver(t)
                pc.mu.Unlock()
            }
            return
        }

        // CreateDataChannel creates a new DataChannel object with the given label
        // and optional DataChannelInit used to configure properties of the
        // underlying channel such as data reliability.
        func (pc *PeerConnection) CreateDataChannel(label string, options *DataChannelInit) (*DataChannel, error) {
            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #2)
            if pc.is_closed.get() {
                return nil, &rtcerr.InvalidStateError{Err: ErrConnectionClosed}
            }

            params := &DataChannelParameters{
                Label:   label,
                Ordered: true,
            }

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #19)
            if options != nil {
                params.ID = options.ID
            }

            if options != nil {
                // Ordered indicates if data is allowed to be delivered out of order. The
                // default value of true, guarantees that data will be delivered in order.
                // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #9)
                if options.Ordered != nil {
                    params.Ordered = *options.Ordered
                }

                // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #7)
                if options.MaxPacketLifeTime != nil {
                    params.MaxPacketLifeTime = options.MaxPacketLifeTime
                }

                // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #8)
                if options.MaxRetransmits != nil {
                    params.MaxRetransmits = options.MaxRetransmits
                }

                // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #10)
                if options.Protocol != nil {
                    params.Protocol = *options.Protocol
                }

                // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #11)
                if len(params.Protocol) > 65535 {
                    return nil, &rtcerr.TypeError{Err: ErrProtocolTooLarge}
                }

                // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #12)
                if options.Negotiated != nil {
                    params.Negotiated = *options.Negotiated
                }
            }

            d, err := pc.api.newDataChannel(params, pc.log)
            if err != nil {
                return nil, err
            }

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #16)
            if d.maxPacketLifeTime != nil && d.maxRetransmits != nil {
                return nil, &rtcerr.TypeError{Err: ErrRetransmitsOrPacketLifeTime}
            }

            pc.sctpTransport.lock.Lock()
            pc.sctpTransport.dataChannels = append(pc.sctpTransport.dataChannels, d)
            pc.sctpTransport.dataChannelsRequested++
            pc.sctpTransport.lock.Unlock()

            // If SCTP already connected open all the channels
            if pc.sctpTransport.State() == SCTPTransportStateConnected {
                if err = d.open(pc.sctpTransport); err != nil {
                    return nil, err
                }
            }

            pc.mu.Lock()
            pc.onNegotiationNeeded()
            pc.mu.Unlock()

            return d, nil
        }

        // SetIdentityProvider is used to configure an identity provider to generate identity assertions
        func (pc *PeerConnection) SetIdentityProvider(provider string) error {
            return errPeerConnSetIdentityProviderNotImplemented
        }

        // WriteRTCP sends a user provided RTCP packet to the connected peer. If no peer is connected the
        // packet is discarded. It also runs any configured interceptors.
        func (pc *PeerConnection) WriteRTCP(pkts []rtcp.Packet) error {
            _, err := pc.interceptorRTCPWriter.Write(pkts, make(interceptor.Attributes))
            return err
        }

        func (pc *PeerConnection) writeRTCP(pkts []rtcp.Packet, _ interceptor.Attributes) (int, error) {
            return pc.dtlsTransport.WriteRTCP(pkts)
        }

        // Close ends the PeerConnection
        func (pc *PeerConnection) Close() error {
            // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #1)
            if pc.is_closed.get() {
                return nil
            }

            // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #2)
            pc.is_closed.set(true)

            // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #3)
            pc.signaling_state.Set(SignalingStateClosed)

            // Try closing everything and collect the errors
            // Shutdown strategy:
            // 1. All Conn close by closing their underlying Conn.
            // 2. A Mux stops this chain. It won't close the underlying
            //    Conn if one of the endpoints is closed down. To
            //    continue the chain the Mux has to be closed.
            closeErrs := make([]error, 4)

            closeErrs = append(closeErrs, pc.api.interceptor.Close())

            // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #4)
            pc.mu.Lock()
            for _, t := range pc.rtp_transceivers {
                if !t.stopped {
                    closeErrs = append(closeErrs, t.Stop())
                }
            }
            pc.mu.Unlock()

            // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #5)
            pc.sctpTransport.lock.Lock()
            for _, d := range pc.sctpTransport.dataChannels {
                d.setReadyState(DataChannelStateClosed)
            }
            pc.sctpTransport.lock.Unlock()

            // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #6)
            if pc.sctpTransport != nil {
                closeErrs = append(closeErrs, pc.sctpTransport.Stop())
            }

            // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #7)
            closeErrs = append(closeErrs, pc.dtlsTransport.Stop())

            // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #8, #9, #10)
            if pc.iceTransport != nil {
                closeErrs = append(closeErrs, pc.iceTransport.Stop())
            }

            // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #11)
            pc.update_connection_state(pc.ICEConnectionState(), pc.dtlsTransport.State())

            return util.FlattenErrs(closeErrs)
        }

        // addRTPTransceiver appends t into rtp_transceivers
        // and fires onNegotiationNeeded;
        // caller of this method should hold `pc.mu` lock
        func (pc *PeerConnection) addRTPTransceiver(t *RTPTransceiver) {
            pc.rtp_transceivers = append(pc.rtp_transceivers, t)
            pc.onNegotiationNeeded()
        }

        // CurrentLocalDescription represents the local description that was
        // successfully negotiated the last time the PeerConnection transitioned
        // into the stable state plus any local candidates that have been generated
        // by the ICEAgent since the offer or answer was created.
        func (pc *PeerConnection) CurrentLocalDescription() *SessionDescription {
            pc.mu.Lock()
            localDescription := pc.current_local_description
            iceGather := pc.iceGatherer
            iceGatheringState := pc.ICEGatheringState()
            pc.mu.Unlock()
            return populate_local_candidates(localDescription, iceGather, iceGatheringState)
        }

        // PendingLocalDescription represents a local description that is in the
        // process of being negotiated plus any local candidates that have been
        // generated by the ICEAgent since the offer or answer was created. If the
        // PeerConnection is in the stable state, the value is null.
        func (pc *PeerConnection) PendingLocalDescription() *SessionDescription {
            pc.mu.Lock()
            localDescription := pc.pending_local_description
            iceGather := pc.iceGatherer
            iceGatheringState := pc.ICEGatheringState()
            pc.mu.Unlock()
            return populate_local_candidates(localDescription, iceGather, iceGatheringState)
        }

        // CurrentRemoteDescription represents the last remote description that was
        // successfully negotiated the last time the PeerConnection transitioned
        // into the stable state plus any remote candidates that have been supplied
        // via AddICECandidate() since the offer or answer was created.
        func (pc *PeerConnection) CurrentRemoteDescription() *SessionDescription {
            pc.mu.RLock()
            defer pc.mu.RUnlock()

            return pc.current_remote_description
        }

        // PendingRemoteDescription represents a remote description that is in the
        // process of being negotiated, complete with any remote candidates that
        // have been supplied via AddICECandidate() since the offer or answer was
        // created. If the PeerConnection is in the stable state, the value is
        // null.
        func (pc *PeerConnection) PendingRemoteDescription() *SessionDescription {
            pc.mu.RLock()
            defer pc.mu.RUnlock()

            return pc.pending_remote_description
        }
    */
    /// signaling_state attribute returns the signaling state of the
    /// PeerConnection instance.
    pub fn signaling_state(&self) -> SignalingState {
        self.signaling_state.load(Ordering::SeqCst).into()
    }
    /*
        // ICEGatheringState attribute returns the ICE gathering state of the
        // PeerConnection instance.
        func (pc *PeerConnection) ICEGatheringState() ICEGatheringState {
            if pc.iceGatherer == nil {
                return ICEGatheringStateNew
            }

            switch pc.iceGatherer.State() {
            case ICEGathererStateNew:
                return ICEGatheringStateNew
            case ICEGathererStateGathering:
                return ICEGatheringStateGathering
            default:
                return ICEGatheringStateComplete
            }
        }

        // ConnectionState attribute returns the connection state of the
        // PeerConnection instance.
        func (pc *PeerConnection) ConnectionState() PeerConnectionState {
            pc.mu.Lock()
            defer pc.mu.Unlock()

            return pc.connection_state
        }

        // GetStats return data providing statistics about the overall connection
        func (pc *PeerConnection) GetStats() StatsReport {
            var (
                dataChannelsAccepted  uint32
                dataChannelsClosed    uint32
                dataChannelsOpened    uint32
                dataChannelsRequested uint32
            )
            statsCollector := newStatsReportCollector()
            statsCollector.Collecting()

            pc.mu.Lock()
            if pc.iceGatherer != nil {
                pc.iceGatherer.collectStats(statsCollector)
            }
            if pc.iceTransport != nil {
                pc.iceTransport.collectStats(statsCollector)
            }

            pc.sctpTransport.lock.Lock()
            dataChannels := append([]*DataChannel{}, pc.sctpTransport.dataChannels...)
            dataChannelsAccepted = pc.sctpTransport.dataChannelsAccepted
            dataChannelsOpened = pc.sctpTransport.dataChannelsOpened
            dataChannelsRequested = pc.sctpTransport.dataChannelsRequested
            pc.sctpTransport.lock.Unlock()

            for _, d := range dataChannels {
                state := d.ReadyState()
                if state != DataChannelStateConnecting && state != DataChannelStateOpen {
                    dataChannelsClosed++
                }

                d.collectStats(statsCollector)
            }
            pc.sctpTransport.collectStats(statsCollector)

            stats := PeerConnectionStats{
                Timestamp:             statsTimestampNow(),
                Type:                  StatsTypePeerConnection,
                ID:                    pc.stats_id,
                DataChannelsAccepted:  dataChannelsAccepted,
                DataChannelsClosed:    dataChannelsClosed,
                DataChannelsOpened:    dataChannelsOpened,
                DataChannelsRequested: dataChannelsRequested,
            }

            statsCollector.Collect(stats.ID, stats)

            certificates := pc.configuration.Certificates
            for _, certificate := range certificates {
                if err := certificate.collectStats(statsCollector); err != nil {
                    continue
                }
            }
            pc.mu.Unlock()

            pc.api.mediaEngine.collectStats(statsCollector)

            return statsCollector.Ready()
        }

        // Start all transports. PeerConnection now has enough state
        func (pc *PeerConnection) startTransports(iceRole ICERole, dtlsRole DTLSRole, remoteUfrag, remotePwd, fingerprint, fingerprintHash string) {
            // Start the ice transport
            err := pc.iceTransport.Start(
                pc.iceGatherer,
                ICEParameters{
                    UsernameFragment: remoteUfrag,
                    Password:         remotePwd,
                    ICELite:          false,
                },
                &iceRole,
            )
            if err != nil {
                pc.log.Warnf("Failed to start manager: %s", err)
                return
            }

            // Start the dtls_transport transport
            err = pc.dtlsTransport.Start(DTLSParameters{
                Role:         dtlsRole,
                Fingerprints: []DTLSFingerprint{{Algorithm: fingerprintHash, Value: fingerprint}},
            })
            pc.update_connection_state(pc.ICEConnectionState(), pc.dtlsTransport.State())
            if err != nil {
                pc.log.Warnf("Failed to start manager: %s", err)
                return
            }
        }

        func (pc *PeerConnection) startRTP(isRenegotiation bool, remoteDesc *SessionDescription, currentTransceivers []*RTPTransceiver) {
            TrackDetails := track_details_from_sdp(pc.log, remoteDesc.parsed)
            if isRenegotiation {
                for _, t := range currentTransceivers {
                    if t.Receiver() == nil || t.Receiver().Track() == nil {
                        continue
                    }

                    t.Receiver().Track().mu.Lock()
                    ssrc := t.Receiver().Track().ssrc

                    if details := track_details_for_ssrc(TrackDetails, ssrc); details != nil {
                        t.Receiver().Track().id = details.id
                        t.Receiver().Track().streamID = details.streamID
                        t.Receiver().Track().mu.Unlock()
                        continue
                    }

                    t.Receiver().Track().mu.Unlock()

                    if err := t.Receiver().Stop(); err != nil {
                        pc.log.Warnf("Failed to stop RtpReceiver: %s", err)
                        continue
                    }

                    receiver, err := pc.api.new_rtpreceiver(t.Receiver().kind, pc.dtlsTransport)
                    if err != nil {
                        pc.log.Warnf("Failed to create new RtpReceiver: %s", err)
                        continue
                    }
                    t.setReceiver(receiver)
                }
            }

            pc.startRTPReceivers(TrackDetails, currentTransceivers)
            if have_application_media_section(remoteDesc.parsed) {
                pc.startSCTP()
            }

            if !isRenegotiation {
                pc.undeclaredMediaProcessor()
            }
        }

        // generateUnmatchedSDP generates an SDP that doesn't take remote state into account
        // This is used for the initial call for CreateOffer
        func (pc *PeerConnection) generateUnmatchedSDP(transceivers []*RTPTransceiver, useIdentity bool) (*sdp.SessionDescription, error) {
            d, err := sdp.NewJSEPSessionDescription(useIdentity)
            if err != nil {
                return nil, err
            }

            iceParams, err := pc.iceGatherer.GetLocalParameters()
            if err != nil {
                return nil, err
            }

            candidates, err := pc.iceGatherer.GetLocalCandidates()
            if err != nil {
                return nil, err
            }

            isPlanB := pc.configuration.SDPSemantics == SDPSemanticsPlanB
            mediaSections := []mediaSection{}

            // Needed for pc.sctpTransport.dataChannelsRequested
            pc.sctpTransport.lock.Lock()
            defer pc.sctpTransport.lock.Unlock()

            if isPlanB {
                video := make([]*RTPTransceiver, 0)
                audio := make([]*RTPTransceiver, 0)

                for _, t := range transceivers {
                    if t.kind == RTPCodecTypeVideo {
                        video = append(video, t)
                    } else if t.kind == RTPCodecTypeAudio {
                        audio = append(audio, t)
                    }
                    if t.Sender() != nil {
                        t.Sender().setNegotiated()
                    }
                }

                if len(video) > 0 {
                    mediaSections = append(mediaSections, mediaSection{id: "video", transceivers: video})
                }
                if len(audio) > 0 {
                    mediaSections = append(mediaSections, mediaSection{id: "audio", transceivers: audio})
                }

                if pc.sctpTransport.dataChannelsRequested != 0 {
                    mediaSections = append(mediaSections, mediaSection{id: "data", data: true})
                }
            } else {
                for _, t := range transceivers {
                    if t.Sender() != nil {
                        t.Sender().setNegotiated()
                    }
                    mediaSections = append(mediaSections, mediaSection{id: t.Mid(), transceivers: []*RTPTransceiver{t}})
                }

                if pc.sctpTransport.dataChannelsRequested != 0 {
                    mediaSections = append(mediaSections, mediaSection{id: strconv.Itoa(len(mediaSections)), data: true})
                }
            }

            dtlsFingerprints, err := pc.configuration.Certificates[0].GetFingerprints()
            if err != nil {
                return nil, err
            }

            return populate_sdp(d, isPlanB, dtlsFingerprints, pc.api.settingEngine.sdpMediaLevelFingerprints, pc.api.settingEngine.candidates.ICELite, pc.api.mediaEngine, connectionRoleFromDtlsRole(defaultDtlsRoleOffer), candidates, iceParams, mediaSections, pc.ICEGatheringState())
        }

        // generateMatchedSDP generates a SDP and takes the remote state into account
        // this is used everytime we have a RemoteDescription
        // nolint: gocyclo
        func (pc *PeerConnection) generateMatchedSDP(transceivers []*RTPTransceiver, useIdentity bool, includeUnmatched bool, connectionRole sdp.ConnectionRole) (*sdp.SessionDescription, error) { //nolint:gocognit
            d, err := sdp.NewJSEPSessionDescription(useIdentity)
            if err != nil {
                return nil, err
            }

            iceParams, err := pc.iceGatherer.GetLocalParameters()
            if err != nil {
                return nil, err
            }

            candidates, err := pc.iceGatherer.GetLocalCandidates()
            if err != nil {
                return nil, err
            }

            var t *RTPTransceiver
            remoteDescription := pc.current_remote_description
            if pc.pending_remote_description != nil {
                remoteDescription = pc.pending_remote_description
            }
            localTransceivers := append([]*RTPTransceiver{}, transceivers...)
            detectedPlanB := description_is_plan_b(remoteDescription)
            mediaSections := []mediaSection{}
            alreadyHaveApplicationMediaSection := false
            for _, media := range remoteDescription.parsed.MediaDescriptions {
                midValue := getMidValue(media)
                if midValue == "" {
                    return nil, errPeerConnRemoteDescriptionWithoutMidValue
                }

                if media.MediaName.Media == mediaSectionApplication {
                    mediaSections = append(mediaSections, mediaSection{id: midValue, data: true})
                    alreadyHaveApplicationMediaSection = true
                    continue
                }

                kind := NewRTPCodecType(media.MediaName.Media)
                direction := get_peer_direction(media)
                if kind == 0 || direction == RTPTransceiverDirection(Unknown) {
                    continue
                }

                sdpSemantics := pc.configuration.SDPSemantics

                switch {
                case sdpSemantics == SDPSemanticsPlanB || sdpSemantics == SDPSemanticsUnifiedPlanWithFallback && detectedPlanB:
                    if !detectedPlanB {
                        return nil, &rtcerr.TypeError{Err: ErrIncorrectSDPSemantics}
                    }
                    // If we're responding to a plan-b offer, then we should try to fill up this
                    // media entry with all matching local transceivers
                    mediaTransceivers := []*RTPTransceiver{}
                    for {
                        // keep going until we can't get any more
                        t, localTransceivers = satisfyTypeAndDirection(kind, direction, localTransceivers)
                        if t == nil {
                            if len(mediaTransceivers) == 0 {
                                t = &RTPTransceiver{kind: kind}
                                t.setDirection(RTPTransceiverDirectionInactive)
                                mediaTransceivers = append(mediaTransceivers, t)
                            }
                            break
                        }
                        if t.Sender() != nil {
                            t.Sender().setNegotiated()
                        }
                        mediaTransceivers = append(mediaTransceivers, t)
                    }
                    mediaSections = append(mediaSections, mediaSection{id: midValue, transceivers: mediaTransceivers})
                case sdpSemantics == SDPSemanticsUnifiedPlan || sdpSemantics == SDPSemanticsUnifiedPlanWithFallback:
                    if detectedPlanB {
                        return nil, &rtcerr.TypeError{Err: ErrIncorrectSDPSemantics}
                    }
                    t, localTransceivers = findByMid(midValue, localTransceivers)
                    if t == nil {
                        return nil, fmt.Errorf("%w: %q", errPeerConnTranscieverMidNil, midValue)
                    }
                    if t.Sender() != nil {
                        t.Sender().setNegotiated()
                    }
                    mediaTransceivers := []*RTPTransceiver{t}
                    mediaSections = append(mediaSections, mediaSection{id: midValue, transceivers: mediaTransceivers, ridMap: getRids(media)})
                }
            }

            // If we are offering also include unmatched local transceivers
            if includeUnmatched {
                if !detectedPlanB {
                    for _, t := range localTransceivers {
                        if t.Sender() != nil {
                            t.Sender().setNegotiated()
                        }
                        mediaSections = append(mediaSections, mediaSection{id: t.Mid(), transceivers: []*RTPTransceiver{t}})
                    }
                }

                if pc.sctpTransport.dataChannelsRequested != 0 && !alreadyHaveApplicationMediaSection {
                    if detectedPlanB {
                        mediaSections = append(mediaSections, mediaSection{id: "data", data: true})
                    } else {
                        mediaSections = append(mediaSections, mediaSection{id: strconv.Itoa(len(mediaSections)), data: true})
                    }
                }
            }

            if pc.configuration.SDPSemantics == SDPSemanticsUnifiedPlanWithFallback && detectedPlanB {
                pc.log.Info("Plan-B Offer detected; responding with Plan-B Answer")
            }

            dtlsFingerprints, err := pc.configuration.Certificates[0].GetFingerprints()
            if err != nil {
                return nil, err
            }

            return populate_sdp(d, detectedPlanB, dtlsFingerprints, pc.api.settingEngine.sdpMediaLevelFingerprints, pc.api.settingEngine.candidates.ICELite, pc.api.mediaEngine, connectionRole, candidates, iceParams, mediaSections, pc.ICEGatheringState())
        }
    */
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
}
