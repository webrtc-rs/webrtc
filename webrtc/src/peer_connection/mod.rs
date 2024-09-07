#[cfg(test)]
pub(crate) mod peer_connection_test;

/// Custom media-related options, such as `voice_activity_detection`, which are negotiated while establishing connection.
pub mod offer_answer_options;

/// [`RTCSessionDescription`] - wrapper for SDP text and negotiations stage ([`RTCSdpType`]: offer - pranswer - answer - rollback).
pub mod sdp;

pub mod certificate;
pub mod configuration;
pub(crate) mod operation;
mod peer_connection_internal;
pub mod peer_connection_state;
pub mod policy;
pub mod signaling_state;

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ::ice::candidate::candidate_base::unmarshal_candidate;
use ::ice::candidate::Candidate;
use ::sdp::description::session::*;
use ::sdp::util::ConnectionRole;
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use interceptor::{stats, Attributes, Interceptor, RTCPWriter};
use peer_connection_internal::*;
use portable_atomic::{AtomicBool, AtomicU64, AtomicU8};
use rand::{thread_rng, Rng};
use rcgen::KeyPair;
use smol_str::SmolStr;
use srtp::stream::Stream;
use tokio::sync::{mpsc, Mutex};

use crate::api::media_engine::MediaEngine;
use crate::api::setting_engine::SettingEngine;
use crate::api::API;
use crate::data_channel::data_channel_init::RTCDataChannelInit;
use crate::data_channel::data_channel_parameters::DataChannelParameters;
use crate::data_channel::data_channel_state::RTCDataChannelState;
use crate::data_channel::RTCDataChannel;
use crate::dtls_transport::dtls_fingerprint::RTCDtlsFingerprint;
use crate::dtls_transport::dtls_parameters::DTLSParameters;
use crate::dtls_transport::dtls_role::{
    DTLSRole, DEFAULT_DTLS_ROLE_ANSWER, DEFAULT_DTLS_ROLE_OFFER,
};
use crate::dtls_transport::dtls_transport_state::RTCDtlsTransportState;
use crate::dtls_transport::RTCDtlsTransport;
use crate::error::{flatten_errs, Error, Result};
use crate::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use crate::ice_transport::ice_connection_state::RTCIceConnectionState;
use crate::ice_transport::ice_gatherer::{
    OnGatheringCompleteHdlrFn, OnICEGathererStateChangeHdlrFn, OnLocalCandidateHdlrFn,
    RTCIceGatherOptions, RTCIceGatherer,
};
use crate::ice_transport::ice_gatherer_state::RTCIceGathererState;
use crate::ice_transport::ice_gathering_state::RTCIceGatheringState;
use crate::ice_transport::ice_parameters::RTCIceParameters;
use crate::ice_transport::ice_role::RTCIceRole;
use crate::ice_transport::ice_transport_state::RTCIceTransportState;
use crate::ice_transport::RTCIceTransport;
use crate::peer_connection::certificate::RTCCertificate;
use crate::peer_connection::configuration::RTCConfiguration;
use crate::peer_connection::offer_answer_options::{RTCAnswerOptions, RTCOfferOptions};
use crate::peer_connection::operation::{Operation, Operations};
use crate::peer_connection::peer_connection_state::{
    NegotiationNeededState, RTCPeerConnectionState,
};
use crate::peer_connection::sdp::sdp_type::RTCSdpType;
use crate::peer_connection::sdp::session_description::RTCSessionDescription;
use crate::peer_connection::sdp::*;
use crate::peer_connection::signaling_state::{
    check_next_signaling_state, RTCSignalingState, StateChangeOp,
};
use crate::rtp_transceiver::rtp_codec::{RTCRtpHeaderExtensionCapability, RTPCodecType};
use crate::rtp_transceiver::rtp_receiver::RTCRtpReceiver;
use crate::rtp_transceiver::rtp_sender::RTCRtpSender;
use crate::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use crate::rtp_transceiver::{
    find_by_mid, handle_unknown_rtp_packet, satisfy_type_and_direction, RTCRtpTransceiver,
    RTCRtpTransceiverInit, SSRC,
};
use crate::sctp_transport::sctp_transport_capabilities::SCTPTransportCapabilities;
use crate::sctp_transport::sctp_transport_state::RTCSctpTransportState;
use crate::sctp_transport::RTCSctpTransport;
use crate::stats::StatsReport;
use crate::track::track_local::TrackLocal;
use crate::track::track_remote::TrackRemote;

/// SIMULCAST_PROBE_COUNT is the amount of RTP Packets
/// that handleUndeclaredSSRC will read and try to dispatch from
/// mid and rid values
pub(crate) const SIMULCAST_PROBE_COUNT: usize = 10;

/// SIMULCAST_MAX_PROBE_ROUTINES is how many active routines can be used to probe
/// If the total amount of incoming SSRCes exceeds this new requests will be ignored
pub(crate) const SIMULCAST_MAX_PROBE_ROUTINES: u64 = 25;

pub(crate) const MEDIA_SECTION_APPLICATION: &str = "application";

const RUNES_ALPHA: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// math_rand_alpha generates a mathematical random alphabet sequence of the requested length.
pub fn math_rand_alpha(n: usize) -> String {
    let mut rng = thread_rng();

    let rand_string: String = (0..n)
        .map(|_| {
            let idx = rng.gen_range(0..RUNES_ALPHA.len());
            RUNES_ALPHA[idx] as char
        })
        .collect();

    rand_string
}

pub type OnSignalingStateChangeHdlrFn = Box<
    dyn (FnMut(RTCSignalingState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnICEConnectionStateChangeHdlrFn = Box<
    dyn (FnMut(RTCIceConnectionState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnPeerConnectionStateChangeHdlrFn = Box<
    dyn (FnMut(RTCPeerConnectionState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnDataChannelHdlrFn = Box<
    dyn (FnMut(Arc<RTCDataChannel>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnTrackHdlrFn = Box<
    dyn (FnMut(
            Arc<TrackRemote>,
            Arc<RTCRtpReceiver>,
            Arc<RTCRtpTransceiver>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnNegotiationNeededHdlrFn =
    Box<dyn (FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

#[derive(Clone)]
struct StartTransportsParams {
    ice_transport: Arc<RTCIceTransport>,
    dtls_transport: Arc<RTCDtlsTransport>,
    on_peer_connection_state_change_handler: Arc<Mutex<Option<OnPeerConnectionStateChangeHdlrFn>>>,
    is_closed: Arc<AtomicBool>,
    peer_connection_state: Arc<AtomicU8>,
    ice_connection_state: Arc<AtomicU8>,
}

#[derive(Clone)]
struct CheckNegotiationNeededParams {
    sctp_transport: Arc<RTCSctpTransport>,
    rtp_transceivers: Arc<Mutex<Vec<Arc<RTCRtpTransceiver>>>>,
    current_local_description: Arc<Mutex<Option<RTCSessionDescription>>>,
    current_remote_description: Arc<Mutex<Option<RTCSessionDescription>>>,
}

#[derive(Clone)]
struct NegotiationNeededParams {
    on_negotiation_needed_handler: Arc<ArcSwapOption<Mutex<OnNegotiationNeededHdlrFn>>>,
    is_closed: Arc<AtomicBool>,
    ops: Arc<Operations>,
    negotiation_needed_state: Arc<AtomicU8>,
    is_negotiation_needed: Arc<AtomicBool>,
    signaling_state: Arc<AtomicU8>,
    check_negotiation_needed_params: CheckNegotiationNeededParams,
}

/// PeerConnection represents a WebRTC connection that establishes a
/// peer-to-peer communications with another PeerConnection instance in a
/// browser, or to another endpoint implementing the required protocols.
pub struct RTCPeerConnection {
    stats_id: String,
    idp_login_url: Option<String>,

    configuration: Mutex<RTCConfiguration>,

    interceptor_rtcp_writer: Arc<dyn RTCPWriter + Send + Sync>,

    interceptor: Arc<dyn Interceptor + Send + Sync>,

    pub(crate) internal: Arc<PeerConnectionInternal>,
}

impl std::fmt::Debug for RTCPeerConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RTCPeerConnection")
            .field("stats_id", &self.stats_id)
            .field("idp_login_url", &self.idp_login_url)
            .field("signaling_state", &self.signaling_state())
            .field("ice_connection_state", &self.ice_connection_state())
            .finish()
    }
}

impl std::fmt::Display for RTCPeerConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(RTCPeerConnection {})", self.stats_id)
    }
}

impl RTCPeerConnection {
    /// creates a PeerConnection with the default codecs and
    /// interceptors.  See register_default_codecs and register_default_interceptors.
    ///
    /// If you wish to customize the set of available codecs or the set of
    /// active interceptors, create a MediaEngine and call api.new_peer_connection
    /// instead of this function.
    pub(crate) async fn new(api: &API, mut configuration: RTCConfiguration) -> Result<Self> {
        RTCPeerConnection::init_configuration(&mut configuration)?;

        let (interceptor, stats_interceptor): (Arc<dyn Interceptor + Send + Sync>, _) = {
            let mut chain = api.interceptor_registry.build_chain("")?;
            let stats_interceptor = stats::make_stats_interceptor("");
            chain.add(stats_interceptor.clone());

            (Arc::new(chain), stats_interceptor)
        };

        let weak_interceptor = Arc::downgrade(&interceptor);
        let (internal, configuration) =
            PeerConnectionInternal::new(api, weak_interceptor, stats_interceptor, configuration)
                .await?;
        let internal_rtcp_writer = Arc::clone(&internal) as Arc<dyn RTCPWriter + Send + Sync>;
        let interceptor_rtcp_writer = interceptor.bind_rtcp_writer(internal_rtcp_writer).await;

        // <https://w3c.github.io/webrtc-pc/#constructor> (Step #2)
        // Some variables defined explicitly despite their implicit zero values to
        // allow better readability to understand what is happening.
        Ok(RTCPeerConnection {
            stats_id: format!(
                "PeerConnection-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
            interceptor,
            interceptor_rtcp_writer,
            internal,
            configuration: Mutex::new(configuration),
            idp_login_url: None,
        })
    }

    /// init_configuration defines validation of the specified Configuration and
    /// its assignment to the internal configuration variable. This function differs
    /// from its set_configuration counterpart because most of the checks do not
    /// include verification statements related to the existing state. Thus the
    /// function describes only minor verification of some the struct variables.
    fn init_configuration(configuration: &mut RTCConfiguration) -> Result<()> {
        let sanitized_ice_servers = configuration.get_ice_servers();
        if !sanitized_ice_servers.is_empty() {
            for server in &sanitized_ice_servers {
                server.validate()?;
            }
        }

        // <https://www.w3.org/TR/webrtc/#constructor> (step #3)
        if !configuration.certificates.is_empty() {
            let now = SystemTime::now();
            for cert in &configuration.certificates {
                cert.expires
                    .duration_since(now)
                    .map_err(|_| Error::ErrCertificateExpired)?;
            }
        } else {
            let kp = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
            let cert = RTCCertificate::from_key_pair(kp)?;
            configuration.certificates = vec![cert];
        };

        Ok(())
    }

    /// on_signaling_state_change sets an event handler which is invoked when the
    /// peer connection's signaling state changes
    pub fn on_signaling_state_change(&self, f: OnSignalingStateChangeHdlrFn) {
        self.internal
            .on_signaling_state_change_handler
            .store(Some(Arc::new(Mutex::new(f))))
    }

    async fn do_signaling_state_change(&self, new_state: RTCSignalingState) {
        log::info!("signaling state changed to {}", new_state);
        if let Some(handler) = &*self.internal.on_signaling_state_change_handler.load() {
            let mut f = handler.lock().await;
            f(new_state).await;
        }
    }

    /// on_data_channel sets an event handler which is invoked when a data
    /// channel message arrives from a remote peer.
    pub fn on_data_channel(&self, f: OnDataChannelHdlrFn) {
        self.internal
            .on_data_channel_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    /// on_negotiation_needed sets an event handler which is invoked when
    /// a change has occurred which requires session negotiation
    pub fn on_negotiation_needed(&self, f: OnNegotiationNeededHdlrFn) {
        self.internal
            .on_negotiation_needed_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    fn do_negotiation_needed_inner(params: &NegotiationNeededParams) -> bool {
        // https://w3c.github.io/webrtc-pc/#updating-the-negotiation-needed-flag
        // non-canon step 1
        let state: NegotiationNeededState = params
            .negotiation_needed_state
            .load(Ordering::SeqCst)
            .into();
        if state == NegotiationNeededState::Run {
            params
                .negotiation_needed_state
                .store(NegotiationNeededState::Queue as u8, Ordering::SeqCst);
            false
        } else if state == NegotiationNeededState::Queue {
            false
        } else {
            params
                .negotiation_needed_state
                .store(NegotiationNeededState::Run as u8, Ordering::SeqCst);
            true
        }
    }
    /// do_negotiation_needed enqueues negotiation_needed_op if necessary
    /// caller of this method should hold `pc.mu` lock
    async fn do_negotiation_needed(params: NegotiationNeededParams) {
        if !RTCPeerConnection::do_negotiation_needed_inner(&params) {
            return;
        }

        let params2 = params.clone();
        let _ = params
            .ops
            .enqueue(Operation::new(
                move || {
                    let params3 = params2.clone();
                    Box::pin(async move { RTCPeerConnection::negotiation_needed_op(params3).await })
                },
                "do_negotiation_needed",
            ))
            .await;
    }

    async fn after_negotiation_needed_op(params: NegotiationNeededParams) -> bool {
        let old_negotiation_needed_state = params.negotiation_needed_state.load(Ordering::SeqCst);

        params
            .negotiation_needed_state
            .store(NegotiationNeededState::Empty as u8, Ordering::SeqCst);

        if old_negotiation_needed_state == NegotiationNeededState::Queue as u8 {
            RTCPeerConnection::do_negotiation_needed_inner(&params)
        } else {
            false
        }
    }

    async fn negotiation_needed_op(params: NegotiationNeededParams) -> bool {
        // Don't run NegotiatedNeeded checks if on_negotiation_needed is not set
        let handler = &*params.on_negotiation_needed_handler.load();
        if handler.is_none() {
            return false;
        }

        // https://www.w3.org/TR/webrtc/#updating-the-negotiation-needed-flag
        // Step 2.1
        if params.is_closed.load(Ordering::SeqCst) {
            return false;
        }
        // non-canon step 2.2
        if !params.ops.is_empty().await {
            //enqueue negotiation_needed_op again by return true
            return true;
        }

        // non-canon, run again if there was a request
        // starting defer(after_do_negotiation_needed(params).await);

        // Step 2.3
        if params.signaling_state.load(Ordering::SeqCst) != RTCSignalingState::Stable as u8 {
            return RTCPeerConnection::after_negotiation_needed_op(params).await;
        }

        // Step 2.4
        if !RTCPeerConnection::check_negotiation_needed(&params.check_negotiation_needed_params)
            .await
        {
            params.is_negotiation_needed.store(false, Ordering::SeqCst);
            return RTCPeerConnection::after_negotiation_needed_op(params).await;
        }

        // Step 2.5
        if params.is_negotiation_needed.load(Ordering::SeqCst) {
            return RTCPeerConnection::after_negotiation_needed_op(params).await;
        }

        // Step 2.6
        params.is_negotiation_needed.store(true, Ordering::SeqCst);

        // Step 2.7
        if let Some(handler) = handler {
            let mut f = handler.lock().await;
            f().await;
        }

        RTCPeerConnection::after_negotiation_needed_op(params).await
    }

    async fn check_negotiation_needed(params: &CheckNegotiationNeededParams) -> bool {
        // To check if negotiation is needed for connection, perform the following checks:
        // Skip 1, 2 steps
        // Step 3
        let current_local_description = {
            let current_local_description = params.current_local_description.lock().await;
            current_local_description.clone()
        };
        let current_remote_description = {
            let current_remote_description = params.current_remote_description.lock().await;
            current_remote_description.clone()
        };

        if let Some(local_desc) = &current_local_description {
            let len_data_channel = {
                let data_channels = params.sctp_transport.data_channels.lock().await;
                data_channels.len()
            };

            if len_data_channel != 0 && have_data_channel(local_desc).is_none() {
                return true;
            }

            let transceivers = params.rtp_transceivers.lock().await;
            for t in &*transceivers {
                // https://www.w3.org/TR/webrtc/#dfn-update-the-negotiation-needed-flag
                // Step 5.1
                // if t.stopping && !t.stopped {
                // 	return true
                // }
                let mid = t.mid();
                let m = mid
                    .as_ref()
                    .and_then(|mid| get_by_mid(mid.as_str(), local_desc));
                // Step 5.2
                if !t.stopped.load(Ordering::SeqCst) {
                    if m.is_none() {
                        return true;
                    }

                    if let Some(m) = m {
                        // Step 5.3.1
                        if t.direction().has_send() {
                            let dmsid = match m.attribute(ATTR_KEY_MSID).and_then(|o| o) {
                                Some(m) => m,
                                None => return true, // doesn't contain a single a=msid line
                            };

                            let sender = t.sender().await;
                            // (...)or the number of MSIDs from the a=msid lines in this m= section,
                            // or the MSID values themselves, differ from what is in
                            // transceiver.sender.[[AssociatedMediaStreamIds]], return true.

                            // TODO: This check should be robuster by storing all streams in the
                            // local description so we can compare all of them. For no we only
                            // consider the first one.

                            let stream_ids = sender.associated_media_stream_ids();
                            // Different number of lines, 1 vs 0
                            if stream_ids.is_empty() {
                                return true;
                            }

                            // different stream id
                            if dmsid.split_whitespace().next() != Some(&stream_ids[0]) {
                                return true;
                            }
                        }
                        match local_desc.sdp_type {
                            RTCSdpType::Offer => {
                                // Step 5.3.2
                                if let Some(remote_desc) = &current_remote_description {
                                    if let Some(rm) = t
                                        .mid()
                                        .and_then(|mid| get_by_mid(mid.as_str(), remote_desc))
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
                            RTCSdpType::Answer => {
                                let remote_desc = match &current_remote_description {
                                    Some(d) => d,
                                    None => return true,
                                };
                                let offered_direction = match t
                                    .mid()
                                    .and_then(|mid| get_by_mid(mid.as_str(), remote_desc))
                                {
                                    Some(d) => {
                                        let dir = get_peer_direction(d);
                                        if dir == RTCRtpTransceiverDirection::Unspecified {
                                            RTCRtpTransceiverDirection::Inactive
                                        } else {
                                            dir
                                        }
                                    }
                                    None => RTCRtpTransceiverDirection::Inactive,
                                };

                                let current_direction = get_peer_direction(m);
                                // Step 5.3.3
                                if current_direction
                                    != t.direction().intersect(offered_direction.reverse())
                                {
                                    return true;
                                }
                            }
                            _ => {}
                        };
                    }
                }
                // Step 5.4
                if t.stopped.load(Ordering::SeqCst) {
                    let search_mid = match t.mid() {
                        Some(mid) => mid,
                        None => return false,
                    };

                    if let Some(remote_desc) = &*params.current_remote_description.lock().await {
                        return get_by_mid(search_mid.as_str(), local_desc).is_some()
                            || get_by_mid(search_mid.as_str(), remote_desc).is_some();
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
    pub fn on_ice_candidate(&self, f: OnLocalCandidateHdlrFn) {
        self.internal.ice_gatherer.on_local_candidate(f)
    }

    /// on_ice_gathering_state_change sets an event handler which is invoked when the
    /// ICE candidate gathering state has changed.
    pub fn on_ice_gathering_state_change(&self, f: OnICEGathererStateChangeHdlrFn) {
        self.internal.ice_gatherer.on_state_change(f)
    }

    /// on_track sets an event handler which is called when remote track
    /// arrives from a remote peer.
    pub fn on_track(&self, f: OnTrackHdlrFn) {
        self.internal
            .on_track_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    fn do_track(
        on_track_handler: Arc<ArcSwapOption<Mutex<OnTrackHdlrFn>>>,
        track: Arc<TrackRemote>,
        receiver: Arc<RTCRtpReceiver>,
        transceiver: Arc<RTCRtpTransceiver>,
    ) {
        log::debug!("got new track: {:?}", track);

        tokio::spawn(async move {
            if let Some(handler) = &*on_track_handler.load() {
                let mut f = handler.lock().await;
                f(track, receiver, transceiver).await;
            } else {
                log::warn!("on_track unset, unable to handle incoming media streams");
            }
        });
    }

    /// on_ice_connection_state_change sets an event handler which is called
    /// when an ICE connection state is changed.
    pub fn on_ice_connection_state_change(&self, f: OnICEConnectionStateChangeHdlrFn) {
        self.internal
            .on_ice_connection_state_change_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    async fn do_ice_connection_state_change(
        handler: &Arc<ArcSwapOption<Mutex<OnICEConnectionStateChangeHdlrFn>>>,
        ice_connection_state: &Arc<AtomicU8>,
        cs: RTCIceConnectionState,
    ) {
        ice_connection_state.store(cs as u8, Ordering::SeqCst);

        log::info!("ICE connection state changed: {}", cs);
        if let Some(handler) = &*handler.load() {
            let mut f = handler.lock().await;
            f(cs).await;
        }
    }

    /// on_peer_connection_state_change sets an event handler which is called
    /// when the PeerConnectionState has changed
    pub fn on_peer_connection_state_change(&self, f: OnPeerConnectionStateChangeHdlrFn) {
        self.internal
            .on_peer_connection_state_change_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    async fn do_peer_connection_state_change(
        handler: &Arc<ArcSwapOption<Mutex<OnPeerConnectionStateChangeHdlrFn>>>,
        cs: RTCPeerConnectionState,
    ) {
        if let Some(handler) = &*handler.load() {
            let mut f = handler.lock().await;
            f(cs).await;
        }
    }

    // set_configuration updates the configuration of this PeerConnection object.
    pub async fn set_configuration(&self, configuration: RTCConfiguration) -> Result<()> {
        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-setconfiguration (step #2)
        let mut config_lock = self.configuration.lock().await;

        if self.internal.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #3)
        if !configuration.peer_identity.is_empty() {
            if configuration.peer_identity != config_lock.peer_identity {
                return Err(Error::ErrModifyingPeerIdentity);
            }
            config_lock.peer_identity = configuration.peer_identity;
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #4)
        if !configuration.certificates.is_empty() {
            if configuration.certificates.len() != config_lock.certificates.len() {
                return Err(Error::ErrModifyingCertificates);
            }

            config_lock.certificates = configuration.certificates;
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #5)

        if configuration.bundle_policy != config_lock.bundle_policy {
            return Err(Error::ErrModifyingBundlePolicy);
        }
        config_lock.bundle_policy = configuration.bundle_policy;

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #6)
        if configuration.rtcp_mux_policy != config_lock.rtcp_mux_policy {
            return Err(Error::ErrModifyingRTCPMuxPolicy);
        }
        config_lock.rtcp_mux_policy = configuration.rtcp_mux_policy;

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #7)
        if configuration.ice_candidate_pool_size != 0 {
            if config_lock.ice_candidate_pool_size != configuration.ice_candidate_pool_size
                && self.local_description().await.is_some()
            {
                return Err(Error::ErrModifyingICECandidatePoolSize);
            }
            config_lock.ice_candidate_pool_size = configuration.ice_candidate_pool_size;
        }

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #8)

        config_lock.ice_transport_policy = configuration.ice_transport_policy;

        // https://www.w3.org/TR/webrtc/#set-the-configuration (step #11)
        if !configuration.ice_servers.is_empty() {
            // https://www.w3.org/TR/webrtc/#set-the-configuration (step #11.3)
            for server in &configuration.ice_servers {
                server.validate()?;
            }
            config_lock.ice_servers = configuration.ice_servers
        }
        Ok(())
    }

    /// get_configuration returns a Configuration object representing the current
    /// configuration of this PeerConnection object. The returned object is a
    /// copy and direct mutation on it will not take affect until set_configuration
    /// has been called with Configuration passed as its only argument.
    /// <https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-getconfiguration>
    pub async fn get_configuration(&self) -> RTCConfiguration {
        let configuration = self.configuration.lock().await;
        configuration.clone()
    }

    pub fn get_stats_id(&self) -> &str {
        self.stats_id.as_str()
    }

    /// create_offer starts the PeerConnection and generates the localDescription
    /// <https://w3c.github.io/webrtc-pc/#dom-rtcpeerconnection-createoffer>
    pub async fn create_offer(
        &self,
        options: Option<RTCOfferOptions>,
    ) -> Result<RTCSessionDescription> {
        let use_identity = self.idp_login_url.is_some();
        if use_identity {
            return Err(Error::ErrIdentityProviderNotImplemented);
        } else if self.internal.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        }

        if let Some(options) = options {
            if options.ice_restart {
                self.internal.ice_transport.restart().await?;
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
            let current_transceivers = {
                let rtp_transceivers = self.internal.rtp_transceivers.lock().await;
                rtp_transceivers.clone()
            };

            // include unmatched local transceivers
            // update the greater mid if the remote description provides a greater one
            {
                let current_remote_description =
                    self.internal.current_remote_description.lock().await;
                if let Some(d) = &*current_remote_description {
                    if let Some(parsed) = &d.parsed {
                        for media in &parsed.media_descriptions {
                            if let Some(mid) = get_mid_value(media) {
                                if mid.is_empty() {
                                    continue;
                                }
                                let numeric_mid = match mid.parse::<isize>() {
                                    Ok(n) => n,
                                    Err(_) => continue,
                                };
                                if numeric_mid > self.internal.greater_mid.load(Ordering::SeqCst) {
                                    self.internal
                                        .greater_mid
                                        .store(numeric_mid, Ordering::SeqCst);
                                }
                            }
                        }
                    }
                }
            }
            for t in &current_transceivers {
                if t.mid().is_some() {
                    continue;
                }

                if let Some(gen) = &self.internal.setting_engine.mid_generator {
                    let current_greatest = self.internal.greater_mid.load(Ordering::SeqCst);
                    let mid = (gen)(current_greatest);

                    // If it's possible to parse the returned mid as numeric, we will update the greater_mid field.
                    if let Ok(numeric_mid) = mid.parse::<isize>() {
                        if numeric_mid > self.internal.greater_mid.load(Ordering::SeqCst) {
                            self.internal
                                .greater_mid
                                .store(numeric_mid, Ordering::SeqCst);
                        }
                    }

                    t.set_mid(SmolStr::from(mid))?;
                } else {
                    let greater_mid = self.internal.greater_mid.fetch_add(1, Ordering::SeqCst);
                    t.set_mid(SmolStr::from(format!("{}", greater_mid + 1)))?;
                }
            }

            let current_remote_description_is_none = {
                let current_remote_description =
                    self.internal.current_remote_description.lock().await;
                current_remote_description.is_none()
            };

            let mut d = if current_remote_description_is_none {
                self.internal
                    .generate_unmatched_sdp(current_transceivers, use_identity)
                    .await?
            } else {
                self.internal
                    .generate_matched_sdp(
                        current_transceivers,
                        use_identity,
                        true, /*includeUnmatched */
                        DEFAULT_DTLS_ROLE_OFFER.to_connection_role(),
                    )
                    .await?
            };

            {
                let mut sdp_origin = self.internal.sdp_origin.lock().await;
                update_sdp_origin(&mut sdp_origin, &mut d);
            }
            let sdp = d.marshal();

            offer = RTCSessionDescription {
                sdp_type: RTCSdpType::Offer,
                sdp,
                parsed: Some(d),
            };

            // Verify local media hasn't changed during offer
            // generation. Recompute if necessary
            if !self.internal.has_local_description_changed(&offer).await {
                break;
            }
            count += 1;
            if count >= 128 {
                return Err(Error::ErrExcessiveRetries);
            }
        }

        {
            let mut last_offer = self.internal.last_offer.lock().await;
            last_offer.clone_from(&offer.sdp);
        }
        Ok(offer)
    }

    /// Update the PeerConnectionState given the state of relevant transports
    /// <https://www.w3.org/TR/webrtc/#rtcpeerconnectionstate-enum>
    async fn update_connection_state(
        on_peer_connection_state_change_handler: &Arc<
            ArcSwapOption<Mutex<OnPeerConnectionStateChangeHdlrFn>>,
        >,
        is_closed: &Arc<AtomicBool>,
        peer_connection_state: &Arc<AtomicU8>,
        ice_connection_state: RTCIceConnectionState,
        dtls_transport_state: RTCDtlsTransportState,
    ) {
        let connection_state =
            // The RTCPeerConnection object's [[IsClosed]] slot is true.
            if is_closed.load(Ordering::SeqCst) {
                RTCPeerConnectionState::Closed
            } else if ice_connection_state == RTCIceConnectionState::Failed || dtls_transport_state == RTCDtlsTransportState::Failed {
                // Any of the RTCIceTransports or RTCDtlsTransports are in a "failed" state.
                RTCPeerConnectionState::Failed
            } else if ice_connection_state == RTCIceConnectionState::Disconnected {
                // Any of the RTCIceTransports or RTCDtlsTransports are in the "disconnected"
                // state and none of them are in the "failed" or "connecting" or "checking" state.
                RTCPeerConnectionState::Disconnected
            } else if (ice_connection_state == RTCIceConnectionState::New || ice_connection_state == RTCIceConnectionState::Closed) &&
                (dtls_transport_state == RTCDtlsTransportState::New || dtls_transport_state == RTCDtlsTransportState::Closed) {
                // None of the previous states apply and all RTCIceTransports are in the "new" or "closed" state,
                // and all RTCDtlsTransports are in the "new" or "closed" state, or there are no transports.
                RTCPeerConnectionState::New
            } else if (ice_connection_state == RTCIceConnectionState::New || ice_connection_state == RTCIceConnectionState::Checking) ||
                (dtls_transport_state == RTCDtlsTransportState::New || dtls_transport_state == RTCDtlsTransportState::Connecting) {
                // None of the previous states apply and any RTCIceTransport is in the "new" or "checking" state or
                // any RTCDtlsTransport is in the "new" or "connecting" state.
                RTCPeerConnectionState::Connecting
            } else if (ice_connection_state == RTCIceConnectionState::Connected || ice_connection_state == RTCIceConnectionState::Completed || ice_connection_state == RTCIceConnectionState::Closed) &&
                (dtls_transport_state == RTCDtlsTransportState::Connected || dtls_transport_state == RTCDtlsTransportState::Closed) {
                // All RTCIceTransports and RTCDtlsTransports are in the "connected", "completed" or "closed"
                // state and all RTCDtlsTransports are in the "connected" or "closed" state.
                RTCPeerConnectionState::Connected
            } else {
                RTCPeerConnectionState::New
            };

        if peer_connection_state.load(Ordering::SeqCst) == connection_state as u8 {
            return;
        }

        log::info!("peer connection state changed: {}", connection_state);
        peer_connection_state.store(connection_state as u8, Ordering::SeqCst);

        RTCPeerConnection::do_peer_connection_state_change(
            on_peer_connection_state_change_handler,
            connection_state,
        )
        .await;
    }

    /// create_answer starts the PeerConnection and generates the localDescription
    pub async fn create_answer(
        &self,
        _options: Option<RTCAnswerOptions>,
    ) -> Result<RTCSessionDescription> {
        let use_identity = self.idp_login_url.is_some();
        let remote_desc = self.remote_description().await;
        let remote_description: RTCSessionDescription;
        if let Some(desc) = remote_desc {
            remote_description = desc;
        } else {
            return Err(Error::ErrNoRemoteDescription);
        }
        if use_identity {
            return Err(Error::ErrIdentityProviderNotImplemented);
        } else if self.internal.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        } else if self.signaling_state() != RTCSignalingState::HaveRemoteOffer
            && self.signaling_state() != RTCSignalingState::HaveLocalPranswer
        {
            return Err(Error::ErrIncorrectSignalingState);
        }

        let mut connection_role = self
            .internal
            .setting_engine
            .answering_dtls_role
            .to_connection_role();
        if connection_role == ConnectionRole::Unspecified {
            connection_role = DEFAULT_DTLS_ROLE_ANSWER.to_connection_role();
            if let Some(parsed) = remote_description.parsed {
                if Self::is_lite_set(&parsed) && !self.internal.setting_engine.candidates.ice_lite {
                    connection_role = DTLSRole::Server.to_connection_role();
                }
            }
        }

        let local_transceivers = self.get_transceivers().await;
        let mut d = self
            .internal
            .generate_matched_sdp(
                local_transceivers,
                use_identity,
                false, /*includeUnmatched */
                connection_role,
            )
            .await?;

        {
            let mut sdp_origin = self.internal.sdp_origin.lock().await;
            update_sdp_origin(&mut sdp_origin, &mut d);
        }
        let sdp = d.marshal();

        let answer = RTCSessionDescription {
            sdp_type: RTCSdpType::Answer,
            sdp,
            parsed: Some(d),
        };

        {
            let mut last_answer = self.internal.last_answer.lock().await;
            last_answer.clone_from(&answer.sdp);
        }
        Ok(answer)
    }

    // 4.4.1.6 Set the SessionDescription
    pub(crate) async fn set_description(
        &self,
        sd: &RTCSessionDescription,
        op: StateChangeOp,
    ) -> Result<()> {
        if self.internal.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        } else if sd.sdp_type == RTCSdpType::Unspecified {
            return Err(Error::ErrPeerConnSDPTypeInvalidValue);
        }

        let next_state = {
            let cur = self.signaling_state();
            let new_sdpdoes_not_match_offer = Error::ErrSDPDoesNotMatchOffer;
            let new_sdpdoes_not_match_answer = Error::ErrSDPDoesNotMatchAnswer;

            match op {
                StateChangeOp::SetLocal => {
                    match sd.sdp_type {
                        // stable->SetLocal(offer)->have-local-offer
                        RTCSdpType::Offer => {
                            let check = {
                                let last_offer = self.internal.last_offer.lock().await;
                                sd.sdp != *last_offer
                            };
                            if check {
                                Err(new_sdpdoes_not_match_offer)
                            } else {
                                let next_state = check_next_signaling_state(
                                    cur,
                                    RTCSignalingState::HaveLocalOffer,
                                    StateChangeOp::SetLocal,
                                    sd.sdp_type,
                                );
                                if next_state.is_ok() {
                                    let mut pending_local_description =
                                        self.internal.pending_local_description.lock().await;
                                    *pending_local_description = Some(sd.clone());
                                }
                                next_state
                            }
                        }
                        // have-remote-offer->SetLocal(answer)->stable
                        // have-local-pranswer->SetLocal(answer)->stable
                        RTCSdpType::Answer => {
                            let check = {
                                let last_answer = self.internal.last_answer.lock().await;
                                sd.sdp != *last_answer
                            };
                            if check {
                                Err(new_sdpdoes_not_match_answer)
                            } else {
                                let next_state = check_next_signaling_state(
                                    cur,
                                    RTCSignalingState::Stable,
                                    StateChangeOp::SetLocal,
                                    sd.sdp_type,
                                );
                                if next_state.is_ok() {
                                    let pending_remote_description = {
                                        let mut pending_remote_description =
                                            self.internal.pending_remote_description.lock().await;
                                        pending_remote_description.take()
                                    };
                                    let _pending_local_description = {
                                        let mut pending_local_description =
                                            self.internal.pending_local_description.lock().await;
                                        pending_local_description.take()
                                    };

                                    {
                                        let mut current_local_description =
                                            self.internal.current_local_description.lock().await;
                                        *current_local_description = Some(sd.clone());
                                    }
                                    {
                                        let mut current_remote_description =
                                            self.internal.current_remote_description.lock().await;
                                        *current_remote_description = pending_remote_description;
                                    }
                                }
                                next_state
                            }
                        }
                        RTCSdpType::Rollback => {
                            let next_state = check_next_signaling_state(
                                cur,
                                RTCSignalingState::Stable,
                                StateChangeOp::SetLocal,
                                sd.sdp_type,
                            );
                            if next_state.is_ok() {
                                let mut pending_local_description =
                                    self.internal.pending_local_description.lock().await;
                                *pending_local_description = None;
                            }
                            next_state
                        }
                        // have-remote-offer->SetLocal(pranswer)->have-local-pranswer
                        RTCSdpType::Pranswer => {
                            let check = {
                                let last_answer = self.internal.last_answer.lock().await;
                                sd.sdp != *last_answer
                            };
                            if check {
                                Err(new_sdpdoes_not_match_answer)
                            } else {
                                let next_state = check_next_signaling_state(
                                    cur,
                                    RTCSignalingState::HaveLocalPranswer,
                                    StateChangeOp::SetLocal,
                                    sd.sdp_type,
                                );
                                if next_state.is_ok() {
                                    let mut pending_local_description =
                                        self.internal.pending_local_description.lock().await;
                                    *pending_local_description = Some(sd.clone());
                                }
                                next_state
                            }
                        }
                        _ => Err(Error::ErrPeerConnStateChangeInvalid),
                    }
                }
                StateChangeOp::SetRemote => {
                    match sd.sdp_type {
                        // stable->SetRemote(offer)->have-remote-offer
                        RTCSdpType::Offer => {
                            let next_state = check_next_signaling_state(
                                cur,
                                RTCSignalingState::HaveRemoteOffer,
                                StateChangeOp::SetRemote,
                                sd.sdp_type,
                            );
                            if next_state.is_ok() {
                                let mut pending_remote_description =
                                    self.internal.pending_remote_description.lock().await;
                                *pending_remote_description = Some(sd.clone());
                            }
                            next_state
                        }
                        // have-local-offer->SetRemote(answer)->stable
                        // have-remote-pranswer->SetRemote(answer)->stable
                        RTCSdpType::Answer => {
                            let next_state = check_next_signaling_state(
                                cur,
                                RTCSignalingState::Stable,
                                StateChangeOp::SetRemote,
                                sd.sdp_type,
                            );
                            if next_state.is_ok() {
                                let pending_local_description = {
                                    let mut pending_local_description =
                                        self.internal.pending_local_description.lock().await;
                                    pending_local_description.take()
                                };

                                let _pending_remote_description = {
                                    let mut pending_remote_description =
                                        self.internal.pending_remote_description.lock().await;
                                    pending_remote_description.take()
                                };

                                {
                                    let mut current_remote_description =
                                        self.internal.current_remote_description.lock().await;
                                    *current_remote_description = Some(sd.clone());
                                }
                                {
                                    let mut current_local_description =
                                        self.internal.current_local_description.lock().await;
                                    *current_local_description = pending_local_description;
                                }
                            }
                            next_state
                        }
                        RTCSdpType::Rollback => {
                            let next_state = check_next_signaling_state(
                                cur,
                                RTCSignalingState::Stable,
                                StateChangeOp::SetRemote,
                                sd.sdp_type,
                            );
                            if next_state.is_ok() {
                                let mut pending_remote_description =
                                    self.internal.pending_remote_description.lock().await;
                                *pending_remote_description = None;
                            }
                            next_state
                        }
                        // have-local-offer->SetRemote(pranswer)->have-remote-pranswer
                        RTCSdpType::Pranswer => {
                            let next_state = check_next_signaling_state(
                                cur,
                                RTCSignalingState::HaveRemotePranswer,
                                StateChangeOp::SetRemote,
                                sd.sdp_type,
                            );
                            if next_state.is_ok() {
                                let mut pending_remote_description =
                                    self.internal.pending_remote_description.lock().await;
                                *pending_remote_description = Some(sd.clone());
                            }
                            next_state
                        }
                        _ => Err(Error::ErrPeerConnStateChangeInvalid),
                    }
                } //_ => Err(Error::ErrPeerConnStateChangeUnhandled.into()),
            }
        };

        match next_state {
            Ok(next_state) => {
                self.internal
                    .signaling_state
                    .store(next_state as u8, Ordering::SeqCst);
                if self.signaling_state() == RTCSignalingState::Stable {
                    self.internal
                        .is_negotiation_needed
                        .store(false, Ordering::SeqCst);
                    self.internal.trigger_negotiation_needed().await;
                }
                self.do_signaling_state_change(next_state).await;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// set_local_description sets the SessionDescription of the local peer
    pub async fn set_local_description(&self, mut desc: RTCSessionDescription) -> Result<()> {
        if self.internal.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        }

        let have_local_description = {
            let current_local_description = self.internal.current_local_description.lock().await;
            current_local_description.is_some()
        };

        // JSEP 5.4
        if desc.sdp.is_empty() {
            match desc.sdp_type {
                RTCSdpType::Answer | RTCSdpType::Pranswer => {
                    let last_answer = self.internal.last_answer.lock().await;
                    desc.sdp.clone_from(&last_answer);
                }
                RTCSdpType::Offer => {
                    let last_offer = self.internal.last_offer.lock().await;
                    desc.sdp.clone_from(&last_offer);
                }
                _ => return Err(Error::ErrPeerConnSDPTypeInvalidValueSetLocalDescription),
            }
        }

        desc.parsed = Some(desc.unmarshal()?);
        self.set_description(&desc, StateChangeOp::SetLocal).await?;

        let we_answer = desc.sdp_type == RTCSdpType::Answer;
        let remote_description = self.remote_description().await;
        let mut local_transceivers = self.get_transceivers().await;
        if we_answer {
            if let Some(parsed) = desc.parsed {
                // WebRTC Spec 1.0 https://www.w3.org/TR/webrtc/
                // Section 4.4.1.5
                for media in &parsed.media_descriptions {
                    if media.media_name.media == MEDIA_SECTION_APPLICATION {
                        continue;
                    }

                    let kind = RTPCodecType::from(media.media_name.media.as_str());
                    let direction = get_peer_direction(media);
                    if kind == RTPCodecType::Unspecified
                        || direction == RTCRtpTransceiverDirection::Unspecified
                    {
                        continue;
                    }

                    let mid_value = match get_mid_value(media) {
                        Some(mid) if !mid.is_empty() => mid,
                        _ => continue,
                    };

                    let t = match find_by_mid(mid_value, &mut local_transceivers).await {
                        Some(t) => t,
                        None => continue,
                    };
                    let previous_direction = t.current_direction();
                    // 4.9.1.7.3 applying a local answer or pranswer
                    // Set transceiver.[[CurrentDirection]] and transceiver.[[FiredDirection]] to direction.

                    // TODO: Also set FiredDirection here.
                    t.set_current_direction(direction);
                    t.process_new_current_direction(previous_direction).await?;
                }
            }

            if let Some(remote_desc) = remote_description {
                self.start_rtp_senders().await?;

                let pci = Arc::clone(&self.internal);
                let remote_desc = Arc::new(remote_desc);
                self.internal
                    .ops
                    .enqueue(Operation::new(
                        move || {
                            let pc = Arc::clone(&pci);
                            let rd = Arc::clone(&remote_desc);
                            Box::pin(async move {
                                let _ = pc.start_rtp(have_local_description, rd).await;
                                false
                            })
                        },
                        "set_local_description",
                    ))
                    .await?;
            }
        }

        if self.internal.ice_gatherer.state() == RTCIceGathererState::New {
            self.internal.ice_gatherer.gather().await
        } else {
            Ok(())
        }
    }

    /// local_description returns PendingLocalDescription if it is not null and
    /// otherwise it returns CurrentLocalDescription. This property is used to
    /// determine if set_local_description has already been called.
    /// <https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-localdescription>
    pub async fn local_description(&self) -> Option<RTCSessionDescription> {
        if let Some(pending_local_description) = self.pending_local_description().await {
            return Some(pending_local_description);
        }
        self.current_local_description().await
    }

    pub fn is_lite_set(desc: &SessionDescription) -> bool {
        for a in &desc.attributes {
            if a.key.trim() == ATTR_KEY_ICELITE {
                return true;
            }
        }
        false
    }

    /// set_remote_description sets the SessionDescription of the remote peer
    pub async fn set_remote_description(&self, mut desc: RTCSessionDescription) -> Result<()> {
        if self.internal.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        }

        let is_renegotiation = {
            let current_remote_description = self.internal.current_remote_description.lock().await;
            current_remote_description.is_some()
        };

        desc.parsed = Some(desc.unmarshal()?);
        self.set_description(&desc, StateChangeOp::SetRemote)
            .await?;

        if let Some(parsed) = &desc.parsed {
            self.internal
                .media_engine
                .update_from_remote_description(parsed)
                .await?;

            let mut local_transceivers = self.get_transceivers().await;
            let remote_description = self.remote_description().await;
            let we_offer = desc.sdp_type == RTCSdpType::Answer;

            if !we_offer {
                if let Some(parsed) = remote_description.as_ref().and_then(|r| r.parsed.as_ref()) {
                    for media in &parsed.media_descriptions {
                        let mid_value = match get_mid_value(media) {
                            Some(m) => {
                                if m.is_empty() {
                                    return Err(Error::ErrPeerConnRemoteDescriptionWithoutMidValue);
                                } else {
                                    m
                                }
                            }
                            None => continue,
                        };

                        if media.media_name.media == MEDIA_SECTION_APPLICATION {
                            continue;
                        }

                        let kind = RTPCodecType::from(media.media_name.media.as_str());
                        let direction = get_peer_direction(media);
                        if kind == RTPCodecType::Unspecified
                            || direction == RTCRtpTransceiverDirection::Unspecified
                        {
                            continue;
                        }

                        let t = if let Some(t) =
                            find_by_mid(mid_value, &mut local_transceivers).await
                        {
                            Some(t)
                        } else {
                            satisfy_type_and_direction(kind, direction, &mut local_transceivers)
                                .await
                        };

                        if let Some(t) = t {
                            if t.mid().is_none() {
                                t.set_mid(SmolStr::from(mid_value))?;
                            }
                        } else {
                            let local_direction =
                                if direction == RTCRtpTransceiverDirection::Recvonly {
                                    RTCRtpTransceiverDirection::Sendonly
                                } else {
                                    RTCRtpTransceiverDirection::Recvonly
                                };

                            let receive_mtu = self.internal.setting_engine.get_receive_mtu();

                            let receiver = Arc::new(RTCRtpReceiver::new(
                                receive_mtu,
                                kind,
                                Arc::clone(&self.internal.dtls_transport),
                                Arc::clone(&self.internal.media_engine),
                                Arc::clone(&self.interceptor),
                            ));

                            let sender = Arc::new(
                                RTCRtpSender::new(
                                    None,
                                    kind,
                                    Arc::clone(&self.internal.dtls_transport),
                                    Arc::clone(&self.internal.media_engine),
                                    Arc::clone(&self.internal.setting_engine),
                                    Arc::clone(&self.interceptor),
                                    false,
                                )
                                .await,
                            );

                            let t = RTCRtpTransceiver::new(
                                receiver,
                                sender,
                                local_direction,
                                kind,
                                vec![],
                                Arc::clone(&self.internal.media_engine),
                                Some(Box::new(self.internal.make_negotiation_needed_trigger())),
                            )
                            .await;

                            self.internal.add_rtp_transceiver(Arc::clone(&t)).await;

                            if t.mid().is_none() {
                                t.set_mid(SmolStr::from(mid_value))?;
                            }
                        }
                    }
                }
            }

            if we_offer {
                // WebRTC Spec 1.0 https://www.w3.org/TR/webrtc/
                // 4.5.9.2
                // This is an answer from the remote.
                if let Some(parsed) = remote_description.as_ref().and_then(|r| r.parsed.as_ref()) {
                    for media in &parsed.media_descriptions {
                        let mid_value = match get_mid_value(media) {
                            Some(m) => {
                                if m.is_empty() {
                                    return Err(Error::ErrPeerConnRemoteDescriptionWithoutMidValue);
                                } else {
                                    m
                                }
                            }
                            None => continue,
                        };

                        if media.media_name.media == MEDIA_SECTION_APPLICATION {
                            continue;
                        }
                        let kind = RTPCodecType::from(media.media_name.media.as_str());
                        let direction = get_peer_direction(media);
                        if kind == RTPCodecType::Unspecified
                            || direction == RTCRtpTransceiverDirection::Unspecified
                        {
                            continue;
                        }

                        if let Some(t) = find_by_mid(mid_value, &mut local_transceivers).await {
                            let previous_direction = t.current_direction();

                            // 4.5.9.2.9
                            // Let direction be an RTCRtpTransceiverDirection value representing the direction
                            // from the media description, but with the send and receive directions reversed to
                            // represent this peer's point of view. If the media description is rejected,
                            // set direction to "inactive".
                            let reversed_direction = direction.reverse();

                            // 4.5.9.2.13.2
                            // Set transceiver.[[CurrentDirection]] and transceiver.[[Direction]]s to direction.
                            t.set_current_direction(reversed_direction);
                            // TODO: According to the specification we should set
                            // transceiver.[[Direction]] here, however libWebrtc doesn't do this.
                            // NOTE: After raising this it seems like the specification might
                            // change to remove the setting of transceiver.[[Direction]].
                            // See https://github.com/w3c/webrtc-pc/issues/2751#issuecomment-1185901962
                            // t.set_direction_internal(reversed_direction);
                            t.process_new_current_direction(previous_direction).await?;
                        }
                    }
                }
            }

            let (remote_ufrag, remote_pwd, candidates) = extract_ice_details(parsed).await?;

            if is_renegotiation
                && self
                    .internal
                    .ice_transport
                    .have_remote_credentials_change(&remote_ufrag, &remote_pwd)
                    .await
            {
                // An ICE Restart only happens implicitly for a set_remote_description of type offer
                if !we_offer {
                    self.internal.ice_transport.restart().await?;
                }

                self.internal
                    .ice_transport
                    .set_remote_credentials(remote_ufrag.clone(), remote_pwd.clone())
                    .await?;
            }

            for candidate in candidates {
                self.internal
                    .ice_transport
                    .add_remote_candidate(Some(candidate))
                    .await?;
            }

            if is_renegotiation {
                if we_offer {
                    self.start_rtp_senders().await?;

                    let pci = Arc::clone(&self.internal);
                    let remote_desc = Arc::new(desc);
                    self.internal
                        .ops
                        .enqueue(Operation::new(
                            move || {
                                let pc = Arc::clone(&pci);
                                let rd = Arc::clone(&remote_desc);
                                Box::pin(async move {
                                    let _ = pc.start_rtp(true, rd).await;
                                    false
                                })
                            },
                            "set_remote_description renegotiation",
                        ))
                        .await?;
                }
                return Ok(());
            }

            let remote_is_lite = Self::is_lite_set(parsed);

            let (fingerprint, fingerprint_hash) = extract_fingerprint(parsed)?;

            // If one of the agents is lite and the other one is not, the lite agent must be the controlling agent.
            // If both or neither agents are lite the offering agent is controlling.
            // RFC 8445 S6.1.1
            let ice_role = if (we_offer
                && remote_is_lite == self.internal.setting_engine.candidates.ice_lite)
                || (remote_is_lite && !self.internal.setting_engine.candidates.ice_lite)
            {
                RTCIceRole::Controlling
            } else {
                RTCIceRole::Controlled
            };

            // Start the networking in a new routine since it will block until
            // the connection is actually established.
            if we_offer {
                self.start_rtp_senders().await?;
            }

            //log::trace!("start_transports: parsed={:?}", parsed);

            let pci = Arc::clone(&self.internal);
            let dtls_role = DTLSRole::from(parsed);
            let remote_desc = Arc::new(desc);
            self.internal
                .ops
                .enqueue(Operation::new(
                    move || {
                        let pc = Arc::clone(&pci);
                        let rd = Arc::clone(&remote_desc);
                        let ru = remote_ufrag.clone();
                        let rp = remote_pwd.clone();
                        let fp = fingerprint.clone();
                        let fp_hash = fingerprint_hash.clone();
                        Box::pin(async move {
                            log::trace!(
                                "start_transports: ice_role={}, dtls_role={}",
                                ice_role,
                                dtls_role,
                            );
                            pc.start_transports(ice_role, dtls_role, ru, rp, fp, fp_hash)
                                .await;

                            if we_offer {
                                let _ = pc.start_rtp(false, rd).await;
                            }
                            false
                        })
                    },
                    "set_remote_description",
                ))
                .await?;
        }

        Ok(())
    }

    /// start_rtp_senders starts all outbound RTP streams
    pub(crate) async fn start_rtp_senders(&self) -> Result<()> {
        let current_transceivers = self.internal.rtp_transceivers.lock().await;
        for transceiver in &*current_transceivers {
            let sender = transceiver.sender().await;
            if !sender.track_encodings.lock().await.is_empty()
                && sender.is_negotiated()
                && !sender.has_sent()
            {
                sender.send(&sender.get_parameters().await).await?;
            }
        }

        Ok(())
    }

    /// remote_description returns pending_remote_description if it is not null and
    /// otherwise it returns current_remote_description. This property is used to
    /// determine if setRemoteDescription has already been called.
    /// <https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-remotedescription>
    pub async fn remote_description(&self) -> Option<RTCSessionDescription> {
        self.internal.remote_description().await
    }

    /// add_ice_candidate accepts an ICE candidate string and adds it
    /// to the existing set of candidates.
    pub async fn add_ice_candidate(&self, candidate: RTCIceCandidateInit) -> Result<()> {
        if self.remote_description().await.is_none() {
            return Err(Error::ErrNoRemoteDescription);
        }

        let candidate_value = match candidate.candidate.strip_prefix("candidate:") {
            Some(s) => s,
            None => candidate.candidate.as_str(),
        };

        let ice_candidate = if !candidate_value.is_empty() {
            let candidate: Arc<dyn Candidate + Send + Sync> =
                Arc::new(unmarshal_candidate(candidate_value)?);

            Some(RTCIceCandidate::from(&candidate))
        } else {
            None
        };

        self.internal
            .ice_transport
            .add_remote_candidate(ice_candidate)
            .await
    }

    /// ice_connection_state returns the ICE connection state of the
    /// PeerConnection instance.
    pub fn ice_connection_state(&self) -> RTCIceConnectionState {
        self.internal
            .ice_connection_state
            .load(Ordering::SeqCst)
            .into()
    }

    /// get_senders returns the RTPSender that are currently attached to this PeerConnection
    pub async fn get_senders(&self) -> Vec<Arc<RTCRtpSender>> {
        let mut senders = vec![];
        let rtp_transceivers = self.internal.rtp_transceivers.lock().await;
        for transceiver in &*rtp_transceivers {
            let sender = transceiver.sender().await;
            senders.push(sender);
        }
        senders
    }

    /// get_receivers returns the RTPReceivers that are currently attached to this PeerConnection
    pub async fn get_receivers(&self) -> Vec<Arc<RTCRtpReceiver>> {
        let mut receivers = vec![];
        let rtp_transceivers = self.internal.rtp_transceivers.lock().await;
        for transceiver in &*rtp_transceivers {
            receivers.push(transceiver.receiver().await);
        }
        receivers
    }

    /// get_transceivers returns the RtpTransceiver that are currently attached to this PeerConnection
    pub async fn get_transceivers(&self) -> Vec<Arc<RTCRtpTransceiver>> {
        let rtp_transceivers = self.internal.rtp_transceivers.lock().await;
        rtp_transceivers.clone()
    }

    /// add_track adds a Track to the PeerConnection
    pub async fn add_track(
        &self,
        track: Arc<dyn TrackLocal + Send + Sync>,
    ) -> Result<Arc<RTCRtpSender>> {
        if self.internal.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        }

        {
            let rtp_transceivers = self.internal.rtp_transceivers.lock().await;
            for t in &*rtp_transceivers {
                if !t.stopped.load(Ordering::SeqCst)
                    && t.kind == track.kind()
                    && t.sender()
                        .await
                        .initial_track_id()
                        .is_some_and(|id| id == track.id())
                {
                    let sender = t.sender().await;
                    if sender.track().await.is_none() {
                        if let Err(err) = sender.replace_track(Some(track)).await {
                            let _ = sender.stop().await;
                            return Err(err);
                        }

                        t.set_direction_internal(RTCRtpTransceiverDirection::from_send_recv(
                            true,
                            t.direction().has_recv(),
                        ));

                        self.internal.trigger_negotiation_needed().await;
                        return Ok(sender);
                    }
                }
            }
        }

        let transceiver = self
            .internal
            .new_transceiver_from_track(RTCRtpTransceiverDirection::Sendrecv, track)
            .await?;
        self.internal
            .add_rtp_transceiver(Arc::clone(&transceiver))
            .await;

        Ok(transceiver.sender().await)
    }

    /// remove_track removes a Track from the PeerConnection
    pub async fn remove_track(&self, sender: &Arc<RTCRtpSender>) -> Result<()> {
        if self.internal.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        }

        let mut transceiver = None;
        {
            let rtp_transceivers = self.internal.rtp_transceivers.lock().await;
            for t in &*rtp_transceivers {
                if t.sender().await.id == sender.id {
                    if sender.track().await.is_none() {
                        return Ok(());
                    }
                    transceiver = Some(t.clone());
                    break;
                }
            }
        }

        let t = transceiver.ok_or(Error::ErrSenderNotCreatedByConnection)?;

        // This also happens in `set_sending_track` but we need to make sure we do this
        // before we call sender.stop to avoid a race condition when removing tracks and
        // generating offers.
        t.set_direction_internal(RTCRtpTransceiverDirection::from_send_recv(
            false,
            t.direction().has_recv(),
        ));
        // Stop the sender
        let sender_result = sender.stop().await;
        // This also updates direction
        let sending_track_result = t.set_sending_track(None).await;

        if sender_result.is_ok() && sending_track_result.is_ok() {
            self.internal.trigger_negotiation_needed().await;
        }
        Ok(())
    }

    /// add_transceiver_from_kind Create a new RtpTransceiver and adds it to the set of transceivers.
    pub async fn add_transceiver_from_kind(
        &self,
        kind: RTPCodecType,
        init: Option<RTCRtpTransceiverInit>,
    ) -> Result<Arc<RTCRtpTransceiver>> {
        self.internal.add_transceiver_from_kind(kind, init).await
    }

    /// add_transceiver_from_track Create a new RtpTransceiver(SendRecv or SendOnly) and add it to the set of transceivers.
    pub async fn add_transceiver_from_track(
        &self,
        track: Arc<dyn TrackLocal + Send + Sync>,
        init: Option<RTCRtpTransceiverInit>,
    ) -> Result<Arc<RTCRtpTransceiver>> {
        if self.internal.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        }

        let direction = init
            .map(|init| init.direction)
            .unwrap_or(RTCRtpTransceiverDirection::Sendrecv);

        let t = self
            .internal
            .new_transceiver_from_track(direction, track)
            .await?;

        self.internal.add_rtp_transceiver(Arc::clone(&t)).await;

        Ok(t)
    }

    /// create_data_channel creates a new DataChannel object with the given label
    /// and optional DataChannelInit used to configure properties of the
    /// underlying channel such as data reliability.
    pub async fn create_data_channel(
        &self,
        label: &str,
        options: Option<RTCDataChannelInit>,
    ) -> Result<Arc<RTCDataChannel>> {
        // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #2)
        if self.internal.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        }

        let mut params = DataChannelParameters {
            label: label.to_owned(),
            ordered: true,
            ..Default::default()
        };

        // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #19)
        if let Some(options) = options {
            // Ordered indicates if data is allowed to be delivered out of order. The
            // default value of true, guarantees that data will be delivered in order.
            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #9)
            if let Some(ordered) = options.ordered {
                params.ordered = ordered;
            }

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #7)
            params.max_packet_life_time = options.max_packet_life_time;

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #8)
            params.max_retransmits = options.max_retransmits;

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #10)
            if let Some(protocol) = options.protocol {
                params.protocol = protocol;
            }

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #11)
            if params.protocol.len() > 65535 {
                return Err(Error::ErrProtocolTooLarge);
            }

            // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #12)
            params.negotiated = options.negotiated;
        }

        let d = Arc::new(RTCDataChannel::new(
            params,
            Arc::clone(&self.internal.setting_engine),
        ));

        // https://w3c.github.io/webrtc-pc/#peer-to-peer-data-api (Step #16)
        if d.max_packet_lifetime.is_some() && d.max_retransmits.is_some() {
            return Err(Error::ErrRetransmitsOrPacketLifeTime);
        }

        {
            let mut data_channels = self.internal.sctp_transport.data_channels.lock().await;
            data_channels.push(Arc::clone(&d));
        }
        self.internal
            .sctp_transport
            .data_channels_requested
            .fetch_add(1, Ordering::SeqCst);

        // If SCTP already connected open all the channels
        if self.internal.sctp_transport.state() == RTCSctpTransportState::Connected {
            d.open(Arc::clone(&self.internal.sctp_transport)).await?;
        }

        self.internal.trigger_negotiation_needed().await;

        Ok(d)
    }

    /// set_identity_provider is used to configure an identity provider to generate identity assertions
    pub fn set_identity_provider(&self, _provider: &str) -> Result<()> {
        Err(Error::ErrPeerConnSetIdentityProviderNotImplemented)
    }

    /// write_rtcp sends a user provided RTCP packet to the connected peer. If no peer is connected the
    /// packet is discarded. It also runs any configured interceptors.
    pub async fn write_rtcp(
        &self,
        pkts: &[Box<dyn rtcp::packet::Packet + Send + Sync>],
    ) -> Result<usize> {
        let a = Attributes::new();
        Ok(self.interceptor_rtcp_writer.write(pkts, &a).await?)
    }

    /// close ends the PeerConnection
    pub async fn close(&self) -> Result<()> {
        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #1)
        if self.internal.is_closed.load(Ordering::SeqCst) {
            return Ok(());
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #2)
        self.internal.is_closed.store(true, Ordering::SeqCst);

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #3)
        self.internal
            .signaling_state
            .store(RTCSignalingState::Closed as u8, Ordering::SeqCst);

        // Try closing everything and collect the errors
        // Shutdown strategy:
        // 1. All Conn close by closing their underlying Conn.
        // 2. A Mux stops this chain. It won't close the underlying
        //    Conn if one of the endpoints is closed down. To
        //    continue the chain the Mux has to be closed.
        let mut close_errs = vec![];

        if let Err(err) = self.interceptor.close().await {
            close_errs.push(Error::new(format!("interceptor: {err}")));
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #4)
        {
            let mut rtp_transceivers = self.internal.rtp_transceivers.lock().await;
            for t in &*rtp_transceivers {
                if let Err(err) = t.stop().await {
                    close_errs.push(Error::new(format!("rtp_transceivers: {err}")));
                }
            }
            rtp_transceivers.clear();
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #5)
        {
            let mut data_channels = self.internal.sctp_transport.data_channels.lock().await;
            for d in &*data_channels {
                if let Err(err) = d.close().await {
                    close_errs.push(Error::new(format!("data_channels: {err}")));
                }
            }
            data_channels.clear();
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #6)
        if let Err(err) = self.internal.sctp_transport.stop().await {
            close_errs.push(Error::new(format!("sctp_transport: {err}")));
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #7)
        if let Err(err) = self.internal.dtls_transport.stop().await {
            close_errs.push(Error::new(format!("dtls_transport: {err}")));
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #8, #9, #10)
        if let Err(err) = self.internal.ice_transport.stop().await {
            close_errs.push(Error::new(format!("ice_transport: {err}")));
        }

        // https://www.w3.org/TR/webrtc/#dom-rtcpeerconnection-close (step #11)
        RTCPeerConnection::update_connection_state(
            &self.internal.on_peer_connection_state_change_handler,
            &self.internal.is_closed,
            &self.internal.peer_connection_state,
            self.ice_connection_state(),
            self.internal.dtls_transport.state(),
        )
        .await;

        if let Err(err) = self.internal.ops.close().await {
            close_errs.push(Error::new(format!("ops: {err}")));
        }

        flatten_errs(close_errs)
    }

    /// CurrentLocalDescription represents the local description that was
    /// successfully negotiated the last time the PeerConnection transitioned
    /// into the stable state plus any local candidates that have been generated
    /// by the ICEAgent since the offer or answer was created.
    pub async fn current_local_description(&self) -> Option<RTCSessionDescription> {
        let local_description = {
            let current_local_description = self.internal.current_local_description.lock().await;
            current_local_description.clone()
        };
        let ice_gather = Some(&self.internal.ice_gatherer);
        let ice_gathering_state = self.ice_gathering_state();

        populate_local_candidates(local_description.as_ref(), ice_gather, ice_gathering_state).await
    }

    /// PendingLocalDescription represents a local description that is in the
    /// process of being negotiated plus any local candidates that have been
    /// generated by the ICEAgent since the offer or answer was created. If the
    /// PeerConnection is in the stable state, the value is null.
    pub async fn pending_local_description(&self) -> Option<RTCSessionDescription> {
        let local_description = {
            let pending_local_description = self.internal.pending_local_description.lock().await;
            pending_local_description.clone()
        };
        let ice_gather = Some(&self.internal.ice_gatherer);
        let ice_gathering_state = self.ice_gathering_state();

        populate_local_candidates(local_description.as_ref(), ice_gather, ice_gathering_state).await
    }

    /// current_remote_description represents the last remote description that was
    /// successfully negotiated the last time the PeerConnection transitioned
    /// into the stable state plus any remote candidates that have been supplied
    /// via add_icecandidate() since the offer or answer was created.
    pub async fn current_remote_description(&self) -> Option<RTCSessionDescription> {
        let current_remote_description = self.internal.current_remote_description.lock().await;
        current_remote_description.clone()
    }

    /// pending_remote_description represents a remote description that is in the
    /// process of being negotiated, complete with any remote candidates that
    /// have been supplied via add_icecandidate() since the offer or answer was
    /// created. If the PeerConnection is in the stable state, the value is
    /// null.
    pub async fn pending_remote_description(&self) -> Option<RTCSessionDescription> {
        let pending_remote_description = self.internal.pending_remote_description.lock().await;
        pending_remote_description.clone()
    }

    /// signaling_state attribute returns the signaling state of the
    /// PeerConnection instance.
    pub fn signaling_state(&self) -> RTCSignalingState {
        self.internal.signaling_state.load(Ordering::SeqCst).into()
    }

    /// icegathering_state attribute returns the ICE gathering state of the
    /// PeerConnection instance.
    pub fn ice_gathering_state(&self) -> RTCIceGatheringState {
        self.internal.ice_gathering_state()
    }

    /// connection_state attribute returns the connection state of the
    /// PeerConnection instance.
    pub fn connection_state(&self) -> RTCPeerConnectionState {
        self.internal
            .peer_connection_state
            .load(Ordering::SeqCst)
            .into()
    }

    pub async fn get_stats(&self) -> StatsReport {
        self.internal
            .get_stats(self.get_stats_id().to_owned())
            .await
            .into()
    }

    /// sctp returns the SCTPTransport for this PeerConnection
    ///
    /// The SCTP transport over which SCTP data is sent and received. If SCTP has not been negotiated, the value is nil.
    /// <https://www.w3.org/TR/webrtc/#attributes-15>
    pub fn sctp(&self) -> Arc<RTCSctpTransport> {
        Arc::clone(&self.internal.sctp_transport)
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
        let done = Arc::new(Mutex::new(Some(gathering_complete_tx)));
        let done2 = Arc::clone(&done);
        self.internal.set_gather_complete_handler(Box::new(move || {
            log::trace!("setGatherCompleteHandler");
            let done3 = Arc::clone(&done2);
            Box::pin(async move {
                let mut d = done3.lock().await;
                d.take();
            })
        }));

        if self.ice_gathering_state() == RTCIceGatheringState::Complete {
            log::trace!("ICEGatheringState::Complete");
            let mut d = done.lock().await;
            d.take();
        }

        gathering_complete_rx
    }

    /// Returns the internal [`RTCDtlsTransport`].
    pub fn dtls_transport(&self) -> Arc<RTCDtlsTransport> {
        Arc::clone(&self.internal.dtls_transport)
    }

    /// Adds the specified [`RTCRtpTransceiver`] to this [`RTCPeerConnection`].
    pub async fn add_transceiver(&self, t: Arc<RTCRtpTransceiver>) {
        self.internal.add_rtp_transceiver(t).await
    }
}
